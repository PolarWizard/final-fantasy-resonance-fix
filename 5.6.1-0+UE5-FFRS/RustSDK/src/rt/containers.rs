//! UE containers laid out exactly like the engine's (x64), with the C++ SDK's API
//! (`Num`, `Max`, `IsValidIndex`, `Add`, `Remove`, `Find`, `[]`, ...) plus Rust iterators.
//! They never allocate or free: copying one copies the pointer, not the elements.

use core::marker::PhantomData;
use core::mem::{size_of, ManuallyDrop};
use core::ops::{Index, IndexMut};
use core::ptr::null_mut;

/// `TArray<T>`
#[repr(C)]
pub struct TArray<T> {
    pub Data: *mut T,
    pub NumElements: i32,
    pub MaxElements: i32,
}

impl<T> Clone for TArray<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for TArray<T> {}

impl<T> Default for TArray<T> {
    fn default() -> Self {
        Self { Data: null_mut(), NumElements: 0, MaxElements: 0 }
    }
}

/// Like the C++ SDK, two arrays are equal if they point to the same data.
impl<T> PartialEq for TArray<T> {
    fn eq(&self, other: &Self) -> bool {
        self.Data == other.Data
    }
}

impl<T> TArray<T> {
    /// An array view over memory you own. The engine must not free or grow it.
    pub const fn from_raw(data: *mut T, num: i32, max: i32) -> Self {
        Self { Data: data, NumElements: num, MaxElements: max }
    }

    pub fn Num(&self) -> i32 {
        self.NumElements
    }

    pub fn Max(&self) -> i32 {
        self.MaxElements
    }

    pub fn GetDataPtr(&self) -> *const T {
        self.Data
    }

    pub fn IsValidIndex(&self, Index: i32) -> bool {
        !self.Data.is_null() && Index >= 0 && Index < self.NumElements
    }

    pub fn IsValid(&self) -> bool {
        !self.Data.is_null() && self.NumElements > 0 && self.MaxElements >= self.NumElements
    }

    /// Adds to the array if there is still space for one more element.
    pub fn Add(&mut self, Element: T) -> bool {
        if self.Data.is_null() || self.MaxElements - self.NumElements <= 0 {
            return false;
        }

        unsafe { self.Data.add(self.NumElements as usize).write(Element) };
        self.NumElements += 1;
        true
    }

    /// Removes the element at `Index`, moving the following elements down.
    pub fn Remove(&mut self, Index: i32) -> bool {
        if !self.IsValidIndex(Index) {
            return false;
        }

        unsafe {
            let at = self.Data.add(Index as usize);
            core::ptr::copy(at.add(1), at, (self.NumElements - Index - 1) as usize);
        }
        self.NumElements -= 1;
        true
    }

    /// Zeroes the elements and sets the count to 0 (the memory isn't freed).
    pub fn Clear(&mut self) {
        if !self.Data.is_null() && self.NumElements > 0 {
            unsafe { core::ptr::write_bytes(self.Data, 0, self.NumElements as usize) };
        }
        self.NumElements = 0;
    }

    pub fn Find(&self, ElementToSearch: &T) -> Option<&T>
    where
        T: PartialEq,
    {
        self.iter().find(|e| *e == ElementToSearch)
    }

    /// `Find` with a custom comparison, like the C++ SDK's `Find(Element, IsEqual)`.
    pub fn FindBy<O: ?Sized>(&self, ElementToSearch: &O, IsEqual: impl Fn(&T, &O) -> bool) -> Option<&T> {
        self.iter().find(|e| IsEqual(e, ElementToSearch))
    }

    pub fn Contains(&self, ElementToSearch: &T) -> bool
    where
        T: PartialEq,
    {
        self.Find(ElementToSearch).is_some()
    }

    pub fn len(&self) -> usize {
        if self.Data.is_null() || self.NumElements < 0 {
            0
        } else {
            self.NumElements as usize
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn as_slice(&self) -> &[T] {
        if self.len() == 0 {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(self.Data, self.len()) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [T] {
        if self.len() == 0 {
            return &mut [];
        }
        unsafe { core::slice::from_raw_parts_mut(self.Data, self.len()) }
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.as_slice().get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.as_mut_slice().get_mut(index)
    }

    pub fn iter(&self) -> core::slice::Iter<'_, T> {
        self.as_slice().iter()
    }

    pub fn iter_mut(&mut self) -> core::slice::IterMut<'_, T> {
        self.as_mut_slice().iter_mut()
    }
}

/// Panics on an invalid index, like the C++ SDK's `operator[]` throws. Use `get` to avoid that.
impl<T> Index<i32> for TArray<T> {
    type Output = T;
    fn index(&self, Index: i32) -> &T {
        assert!(self.IsValidIndex(Index), "Index was out of range!");
        unsafe { &*self.Data.add(Index as usize) }
    }
}

impl<T> IndexMut<i32> for TArray<T> {
    fn index_mut(&mut self, Index: i32) -> &mut T {
        assert!(self.IsValidIndex(Index), "Index was out of range!");
        unsafe { &mut *self.Data.add(Index as usize) }
    }
}

impl<'a, T> IntoIterator for &'a TArray<T> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut TArray<T> {
    type Item = &'a mut T;
    type IntoIter = core::slice::IterMut<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

/// A `TArray` backed by Rust-owned memory, for functions that fill a buffer of a certain size
/// without reallocating it. The memory is freed when this is dropped.
pub struct TAllocatedArray<T> {
    buffer: Vec<T>,
}

impl<T> TAllocatedArray<T> {
    /// An empty array with room for `Size` elements.
    pub fn new(Size: i32) -> Self {
        Self { buffer: Vec::with_capacity(Size.max(0) as usize) }
    }

    /// The `TArray` view to pass to the game. Keep `self` alive while the game uses it.
    pub fn Get(&mut self) -> TArray<T> {
        TArray::from_raw(self.buffer.as_mut_ptr(), self.buffer.len() as i32, self.buffer.capacity() as i32)
    }
}

/// `TPair<K, V>` (also `TTuple<K, V>`)
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct TPair<K, V> {
    pub Key: K,
    pub Value: V,
}

impl<K, V> TPair<K, V> {
    pub fn Key(&self) -> &K {
        &self.Key
    }

    pub fn Value(&self) -> &V {
        &self.Value
    }
}

/// `TBitArray<FDefaultBitArrayAllocator>` (4 inline words, then a heap pointer).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FBitArray {
    pub InlineData: [u32; 4],
    pub SecondaryData: *mut u32,
    pub NumBits: i32,
    pub MaxBits: i32,
}

impl FBitArray {
    pub fn Num(&self) -> i32 {
        self.NumBits
    }

    pub fn IsValid(&self) -> bool {
        self.NumBits >= 0 && self.MaxBits >= self.NumBits
    }

    /// Whether bit `Index` is set.
    pub fn Get(&self, Index: i32) -> bool {
        if Index < 0 || Index >= self.NumBits {
            return false;
        }

        let words = if self.SecondaryData.is_null() { self.InlineData.as_ptr() } else { self.SecondaryData.cast_const() };
        unsafe { *words.add(Index as usize / 32) & (1 << (Index % 32)) != 0 }
    }
}

/// One slot of a `TSparseArray`: either an element or a free-list link.
#[repr(C)]
pub union TSparseArrayElement<T> {
    pub Element: ManuallyDrop<T>,
    pub FreeListLink: [i32; 2],
}

/// `TSparseArray<T>`
#[repr(C)]
pub struct TSparseArray<T> {
    pub Data: TArray<TSparseArrayElement<T>>,
    pub AllocationFlags: FBitArray,
    pub FirstFreeIndex: i32,
    pub NumFreeIndices: i32,
}

impl<T> Clone for TSparseArray<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for TSparseArray<T> {}

impl<T> PartialEq for TSparseArray<T> {
    fn eq(&self, other: &Self) -> bool {
        self.Data == other.Data
    }
}

impl<T> TSparseArray<T> {
    /// Number of slots, including free ones.
    pub fn NumAllocated(&self) -> i32 {
        self.Data.Num()
    }

    /// Number of elements.
    pub fn Num(&self) -> i32 {
        self.NumAllocated() - self.NumFreeIndices
    }

    pub fn Max(&self) -> i32 {
        self.Data.Max()
    }

    /// Whether slot `Index` holds an element.
    pub fn IsValidIndex(&self, Index: i32) -> bool {
        self.Data.IsValidIndex(Index) && self.AllocationFlags.Get(Index)
    }

    pub fn IsValid(&self) -> bool {
        self.Data.IsValid() && self.AllocationFlags.IsValid()
    }

    pub fn GetAllocationFlags(&self) -> &FBitArray {
        &self.AllocationFlags
    }

    /// The element in slot `Index`, if that slot is used.
    pub fn get(&self, Index: i32) -> Option<&T> {
        if !self.IsValidIndex(Index) {
            return None;
        }
        Some(unsafe { &self.Data[Index].Element })
    }

    pub fn get_mut(&mut self, Index: i32) -> Option<&mut T> {
        if !self.IsValidIndex(Index) {
            return None;
        }
        Some(unsafe { &mut self.Data[Index].Element })
    }

    /// Elements in slot order.
    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        (0..self.NumAllocated()).filter_map(move |i| self.get(i))
    }
}

/// Panics if slot `Index` is unused, like the C++ SDK's `operator[]` throws.
impl<T> Index<i32> for TSparseArray<T> {
    type Output = T;
    fn index(&self, Index: i32) -> &T {
        self.get(Index).expect("Index was out of range!")
    }
}

/// Element of a `TSet`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TSetElement<T> {
    pub Value: T,
    pub HashNextId: i32,
    pub HashIndex: i32,
}

/// `TSet<T>`
#[repr(C)]
pub struct TSet<T> {
    pub Elements: TSparseArray<TSetElement<T>>,
    pub HashInline: i32,
    pub HashSecondary: *mut i32,
    pub HashSize: i32,
}

impl<T> Clone for TSet<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for TSet<T> {}

impl<T> PartialEq for TSet<T> {
    fn eq(&self, other: &Self) -> bool {
        self.Elements == other.Elements
    }
}

impl<T> TSet<T> {
    pub fn NumAllocated(&self) -> i32 {
        self.Elements.NumAllocated()
    }

    pub fn Num(&self) -> i32 {
        self.Elements.Num()
    }

    pub fn Max(&self) -> i32 {
        self.Elements.Max()
    }

    pub fn IsValidIndex(&self, Index: i32) -> bool {
        self.Elements.IsValidIndex(Index)
    }

    pub fn IsValid(&self) -> bool {
        self.Elements.IsValid()
    }

    pub fn GetAllocationFlags(&self) -> &FBitArray {
        self.Elements.GetAllocationFlags()
    }

    pub fn get(&self, Index: i32) -> Option<&T> {
        self.Elements.get(Index).map(|e| &e.Value)
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        self.Elements.iter().map(|e| &e.Value)
    }

    pub fn Contains(&self, Value: &T) -> bool
    where
        T: PartialEq,
    {
        self.iter().any(|v| v == Value)
    }
}

impl<T> Index<i32> for TSet<T> {
    type Output = T;
    fn index(&self, Index: i32) -> &T {
        &self.Elements[Index].Value
    }
}

/// `TMap<K, V>`
#[repr(C)]
pub struct TMap<K, V> {
    pub Elements: TSet<TPair<K, V>>,
}

impl<K, V> Clone for TMap<K, V> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<K, V> Copy for TMap<K, V> {}

impl<K, V> PartialEq for TMap<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.Elements == other.Elements
    }
}

impl<K, V> TMap<K, V> {
    pub fn NumAllocated(&self) -> i32 {
        self.Elements.NumAllocated()
    }

    pub fn Num(&self) -> i32 {
        self.Elements.Num()
    }

    pub fn Max(&self) -> i32 {
        self.Elements.Max()
    }

    pub fn IsValidIndex(&self, Index: i32) -> bool {
        self.Elements.IsValidIndex(Index)
    }

    pub fn IsValid(&self) -> bool {
        self.Elements.IsValid()
    }

    pub fn GetAllocationFlags(&self) -> &FBitArray {
        self.Elements.GetAllocationFlags()
    }

    /// The pair with this key (linear search).
    pub fn Find(&self, Key: &K) -> Option<&TPair<K, V>>
    where
        K: PartialEq,
    {
        self.iter().find(|p| p.Key == *Key)
    }

    /// `Find` with a custom key comparison, like the C++ SDK's `Find(Key, Equals)`.
    pub fn FindBy(&self, Key: &K, Equals: impl Fn(&K, &K) -> bool) -> Option<&TPair<K, V>> {
        self.iter().find(|p| Equals(&p.Key, Key))
    }

    /// The value for this key (linear search).
    pub fn get(&self, Key: &K) -> Option<&V>
    where
        K: PartialEq,
    {
        self.Find(Key).map(|p| &p.Value)
    }

    pub fn iter(&self) -> impl Iterator<Item = &TPair<K, V>> + '_ {
        self.Elements.iter()
    }
}

impl<K, V> Index<i32> for TMap<K, V> {
    type Output = TPair<K, V>;
    fn index(&self, Index: i32) -> &TPair<K, V> {
        &self.Elements[Index]
    }
}

/// Raw storage for `T` with `T`'s size but no alignment requirement (used for tail-padding reuse).
#[repr(C)]
pub struct TailPadded<T, const N: usize> {
    pub Bytes: [u8; N],
    _type: PhantomData<*const T>,
}

impl<T, const N: usize> Clone for TailPadded<T, N> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T, const N: usize> Copy for TailPadded<T, N> {}

const _: () = {
    assert!(size_of::<TArray<u8>>() == 0x10);
    assert!(size_of::<FBitArray>() == 0x20);
    assert!(size_of::<TSparseArray<u8>>() == 0x38);
    assert!(size_of::<TSet<u8>>() == 0x50);
    assert!(size_of::<TMap<u8, u8>>() == 0x50);
};
