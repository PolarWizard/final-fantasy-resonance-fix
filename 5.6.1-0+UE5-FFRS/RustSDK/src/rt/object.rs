//! GObjects, object lookup, type checks, and engine/world access, with the C++ SDK's API.

use core::ffi::c_void;
use core::ptr::null_mut;
use core::sync::atomic::{AtomicI32, AtomicPtr, AtomicUsize, Ordering};

use super::{process_event, EClassCastFlags, EObjectFlags, FName, GetImageBase};
use crate::config::{self, UClass, UEngine, UField, UFunction, UGameEngine, UObject, UStruct, UWorld};

/// Address set with `GObjects::InitManually`, replacing the one from `config.rs`.
static GOBJECTS_OVERRIDE: AtomicUsize = AtomicUsize::new(0);

/// The global object array (`FUObjectArray::ObjObjects`, chunked). Use it as `UObject::GObjects`.
#[derive(Clone, Copy)]
pub struct GObjects;

impl GObjects {
    #[inline]
    fn array(self) -> *const u8 {
        match GOBJECTS_OVERRIDE.load(Ordering::Relaxed) {
            0 => (GetImageBase() + config::GOBJECTS) as *const u8,
            address => address as *const u8,
        }
    }

    /// Overrides the GObjects address found by Dumper-7.
    pub fn InitManually(self, GObjectsAddress: *mut c_void) {
        GOBJECTS_OVERRIDE.store(GObjectsAddress as usize, Ordering::Relaxed);
    }

    /// Number of slots (some may be empty).
    pub fn Num(self) -> i32 {
        unsafe { *(self.array().add(config::GOBJECTS_NUM_ELEMENTS) as *const i32) }
    }

    /// The object in slot `Index`, or null.
    pub fn GetByIndex(self, Index: i32) -> *mut UObject {
        if Index < 0 {
            return null_mut();
        }

        unsafe {
            let array = self.array();
            let num_elements = *(array.add(config::GOBJECTS_NUM_ELEMENTS) as *const i32);
            let num_chunks = *(array.add(config::GOBJECTS_NUM_CHUNKS) as *const i32);

            let chunk_index = Index / config::GOBJECTS_CHUNK_SIZE;
            let in_chunk = Index % config::GOBJECTS_CHUNK_SIZE;

            if Index >= num_elements || chunk_index >= num_chunks {
                return null_mut();
            }

            let chunks = config::decrypt_objects(*(array.add(config::GOBJECTS_OBJECTS) as *const *mut u8)) as *const *const u8;
            if chunks.is_null() {
                return null_mut();
            }

            let chunk = *chunks.add(chunk_index as usize);
            if chunk.is_null() {
                return null_mut();
            }

            let item = chunk.add(in_chunk as usize * config::GOBJECTS_ITEM_SIZE + config::GOBJECTS_ITEM_OBJECT_OFFSET);
            *(item as *const *mut UObject)
        }
    }

    /// All non-null objects.
    pub fn iter(self) -> impl Iterator<Item = *mut UObject> {
        (0..self.Num()).map(move |i| self.GetByIndex(i)).filter(|obj| !obj.is_null())
    }
}

/// Implemented by every generated class (via `uclass!`).
///
/// # Safety
/// `static_class` must return the UClass that describes `Self`.
pub unsafe trait UClassType {
    fn static_class() -> *mut UClass;
}

/// What `UObject::IsA` accepts: a class (`*mut UClass`), a class name (`FName`), or `EClassCastFlags`.
pub trait IsAType {
    fn is_a(self, object: &UObject) -> bool;
}

impl IsAType for *mut UClass {
    fn is_a(self, object: &UObject) -> bool {
        object.Class.is_null() == false && unsafe { (*object.Class).IsSubclassOf(self) }
    }
}

impl IsAType for *const UClass {
    fn is_a(self, object: &UObject) -> bool {
        self.cast_mut().is_a(object)
    }
}

impl IsAType for &FName {
    fn is_a(self, object: &UObject) -> bool {
        object.Class.is_null() == false && unsafe { (*object.Class).IsSubclassOf(self) }
    }
}

impl IsAType for FName {
    fn is_a(self, object: &UObject) -> bool {
        (&self).is_a(object)
    }
}

impl IsAType for EClassCastFlags {
    fn is_a(self, object: &UObject) -> bool {
        object.HasTypeFlag(self)
    }
}

/// What `UStruct::IsSubclassOf` accepts: a struct/class pointer or a struct name.
pub trait SubclassOfType {
    fn is_base_of(self, s: &UStruct) -> bool;
}

impl SubclassOfType for *const UStruct {
    fn is_base_of(self, s: &UStruct) -> bool {
        if self.is_null() {
            return false;
        }

        let mut current = s as *const UStruct;
        while !current.is_null() {
            if current == self {
                return true;
            }
            current = unsafe { (*current).SuperStruct };
        }
        false
    }
}

impl SubclassOfType for *mut UStruct {
    fn is_base_of(self, s: &UStruct) -> bool {
        self.cast_const().is_base_of(s)
    }
}

impl SubclassOfType for *mut UClass {
    fn is_base_of(self, s: &UStruct) -> bool {
        self.cast::<UStruct>().cast_const().is_base_of(s)
    }
}

impl SubclassOfType for *const UClass {
    fn is_base_of(self, s: &UStruct) -> bool {
        self.cast::<UStruct>().is_base_of(s)
    }
}

impl SubclassOfType for &FName {
    fn is_base_of(self, s: &UStruct) -> bool {
        if self.IsNone() {
            return false;
        }

        let mut current = s as *const UStruct;
        while !current.is_null() {
            if unsafe { (*current.cast::<UObject>()).Name } == *self {
                return true;
            }
            current = unsafe { (*current).SuperStruct };
        }
        false
    }
}

impl SubclassOfType for FName {
    fn is_base_of(self, s: &UStruct) -> bool {
        (&self).is_base_of(s)
    }
}

/// What `UClass::GetFunction` accepts for class and function names: `&str` or `&FName`.
pub trait FunctionNameType: Copy {
    fn matches(self, object: &UObject) -> bool;
}

impl FunctionNameType for &str {
    fn matches(self, object: &UObject) -> bool {
        object.GetName() == self
    }
}

impl FunctionNameType for &FName {
    fn matches(self, object: &UObject) -> bool {
        object.Name == *self
    }
}

fn find(RequiredType: EClassCastFlags, mut matches: impl FnMut(&UObject) -> bool) -> *mut UObject {
    for obj in GObjects.iter() {
        let o = unsafe { &*obj };
        if o.HasTypeFlag(RequiredType) && matches(o) {
            return obj;
        }
    }
    null_mut()
}

impl UObject {
    /// The global object array: `UObject::GObjects.Num()`, `UObject::GObjects.GetByIndex(i)`.
    pub const GObjects: GObjects = GObjects;

    /// Finds an object by full name (`"Class Engine.Actor"`), optionally with `EClassCastFlags` to
    /// reduce string comparisons. Slow: cache the result.
    pub fn FindObject(FullName: &str, RequiredType: EClassCastFlags) -> *mut UObject {
        find(RequiredType, |o| o.GetFullName() == FullName)
    }

    /// Finds an object by name (`"Actor"`), optionally with `EClassCastFlags`. Slow: cache the result.
    pub fn FindObjectFast(Name: &str, RequiredType: EClassCastFlags) -> *mut UObject {
        find(RequiredType, |o| o.GetName() == Name)
    }

    /// Finds a class by full name, e.g. `"Class Engine.Actor"`.
    pub fn FindClass(ClassFullName: &str) -> *mut UClass {
        Self::FindObject(ClassFullName, EClassCastFlags::Class).cast()
    }

    /// Finds a class by name, e.g. `"Actor"`.
    pub fn FindClassFast(ClassName: &str) -> *mut UClass {
        Self::FindObjectFast(ClassName, EClassCastFlags::Class).cast()
    }

    /// The name of this object, e.g. `PlayerController_0`.
    pub fn GetName(&self) -> String {
        self.Name.ToString()
    }

    /// The name of this object in the format `Class Package.Outer.Object`.
    pub fn GetFullName(&self) -> String {
        if self.Class.is_null() {
            return String::from("None");
        }

        let mut path = String::new();
        let mut outer = self.Outer;
        while !outer.is_null() {
            unsafe {
                path = format!("{}.{}", (*outer).GetName(), path);
                outer = (*outer).Outer;
            }
        }

        let class_name = unsafe { (*self.Class.cast::<UObject>()).GetName() };
        format!("{} {}{}", class_name, path, self.GetName())
    }

    /// Checks this object's type by class (`AActor::StaticClass()`), class name (`ABP_Thing_C::StaticName()`)
    /// or `EClassCastFlags` (fastest, limited to base types).
    pub fn IsA<T: IsAType>(&self, Type: T) -> bool {
        Type.is_a(self)
    }

    /// Whether this object's class has all of `TypeFlags`.
    pub fn HasTypeFlag(&self, TypeFlags: EClassCastFlags) -> bool {
        if self.Class.is_null() {
            return false;
        }
        unsafe { (*self.Class).CastFlags.0 & TypeFlags.0 == TypeFlags.0 }
    }

    /// Checks whether this object is a class's default object.
    pub fn IsDefaultObject(&self) -> bool {
        self.Flags.0 & EObjectFlags::ClassDefaultObject.0 != 0
    }

    /// Unreal function to process all UFunction calls.
    ///
    /// # Safety
    /// `Function` must be a live UFunction and `Parms` its parameter struct (or null). Call it on the game thread.
    pub unsafe fn ProcessEvent(&mut self, Function: *mut UFunction, Parms: *mut c_void) {
        unsafe { process_event(self, Function, Parms) };
    }

    /// `Some` if this object is a `T` (the C++ SDK's `IsA` + `static_cast`).
    pub fn Cast<T: UClassType>(&mut self) -> Option<&mut T> {
        if self.IsA(T::static_class()) {
            Some(unsafe { &mut *(self as *mut Self).cast::<T>() })
        } else {
            None
        }
    }
}

/// `Object` as a `T`, or null if it isn't one.
pub fn Cast<T: UClassType>(Object: *mut UObject) -> *mut T {
    if !Object.is_null() && unsafe { (*Object).IsA(T::static_class()) } {
        Object.cast()
    } else {
        null_mut()
    }
}

impl UStruct {
    /// Checks if this struct/class has a certain base (a `*mut UStruct`/`*mut UClass`, or its name as `FName`).
    pub fn IsSubclassOf<T: SubclassOfType>(&self, Base: T) -> bool {
        Base.is_base_of(self)
    }
}

impl UClass {
    /// Gets a UFunction declared on the class named `ClassName` (this class or a super).
    /// Both names are `&str` or both are `&FName`.
    pub fn GetFunction<N: FunctionNameType>(&self, ClassName: N, FuncName: N) -> *mut UFunction {
        let mut s = self as *const UClass as *const UStruct;

        while !s.is_null() {
            unsafe {
                if ClassName.matches(&*s.cast::<UObject>()) {
                    if let Some(func) = find_function_in(s, |o| FuncName.matches(o)) {
                        return func;
                    }
                }
                s = (*s).SuperStruct;
            }
        }
        null_mut()
    }

    /// Gets the first UFunction named `FuncName` in the class hierarchy (the C++ SDK's `GetFunction(FuncName)`).
    pub fn FindFunction(&self, FuncName: &FName) -> *mut UFunction {
        let mut s = self as *const UClass as *const UStruct;

        while !s.is_null() {
            unsafe {
                if let Some(func) = find_function_in(s, |o| o.Name == *FuncName) {
                    return func;
                }
                s = (*s).SuperStruct;
            }
        }
        null_mut()
    }
}

unsafe fn find_function_in(s: *const UStruct, mut matches: impl FnMut(&UObject) -> bool) -> Option<*mut UFunction> {
    unsafe {
        let mut field: *mut UField = (*s).Children;

        while !field.is_null() {
            let obj = &*field.cast::<UObject>();
            if obj.HasTypeFlag(EClassCastFlags::Function) && matches(obj) {
                return Some(field.cast());
            }
            field = (*field).Next;
        }
    }
    None
}

/// `class->GetFunction(class_name, func_name)` for a raw class pointer (used by generated wrappers).
#[doc(hidden)]
pub unsafe fn FindFunctionInClass(class: *mut UClass, class_name: &str, func_name: &str) -> *mut UFunction {
    if class.is_null() {
        return null_mut();
    }
    unsafe { (*class).GetFunction(class_name, func_name) }
}

/// The class default object of `class`, or null.
#[doc(hidden)]
pub fn default_object(class: *mut UClass) -> *mut UObject {
    if class.is_null() {
        return null_mut();
    }
    unsafe { (*class).ClassDefaultObject }
}

/// Caches a UClass by object index and name, and looks it up again if the slot changes
/// (Blueprint classes can be unloaded and reloaded).
#[doc(hidden)]
pub struct ClassCache {
    name: &'static str,
    full_name: bool,
    index: AtomicI32,
    comparison_index: AtomicI32,
}

impl ClassCache {
    pub const fn new(name: &'static str, full_name: bool) -> Self {
        Self { name, full_name, index: AtomicI32::new(-1), comparison_index: AtomicI32::new(0) }
    }

    pub fn get(&self) -> *mut UClass {
        let index = self.index.load(Ordering::Relaxed);
        if index >= 0 {
            let obj = GObjects.GetByIndex(index);
            if !obj.is_null() && unsafe { (*obj).Name.ComparisonIndex } == self.comparison_index.load(Ordering::Relaxed) {
                return obj.cast();
            }
        }

        let class = if self.full_name { UObject::FindClass(self.name) } else { UObject::FindClassFast(self.name) };
        if !class.is_null() {
            let obj = unsafe { &*class.cast::<UObject>() };
            self.comparison_index.store(obj.Name.ComparisonIndex, Ordering::Relaxed);
            self.index.store(obj.Index, Ordering::Relaxed);
        }
        class
    }
}

impl UEngine {
    /// Gets a pointer to a valid UObject of type UEngine.
    pub fn GetEngine() -> *mut UEngine {
        static ENGINE: AtomicPtr<UObject> = AtomicPtr::new(null_mut());

        let cached = ENGINE.load(Ordering::Relaxed);
        if !cached.is_null() {
            return cached.cast();
        }

        let engine_class = UObject::FindClassFast("Engine");
        if engine_class.is_null() {
            return null_mut();
        }

        for obj in GObjects.iter() {
            let o = unsafe { &*obj };
            if o.IsA(engine_class) && !o.IsDefaultObject() {
                ENGINE.store(obj, Ordering::Relaxed);
                return obj.cast();
            }
        }
        null_mut()
    }
}

impl UGameEngine {
    /// Returns the result of `UEngine::GetEngine()` without a type check.
    pub fn GetEngine() -> *mut UGameEngine {
        UEngine::GetEngine().cast()
    }
}

impl UWorld {
    /// Gets a pointer to the current World: `GWorld` if its offset was found, otherwise `GEngine->GameViewport->World`.
    pub fn GetWorld() -> *mut UWorld {
        unsafe {
            if config::GWORLD != 0 {
                return *((GetImageBase() + config::GWORLD) as *const *mut UWorld);
            }

            let engine = UEngine::GetEngine();
            if engine.is_null() || config::ENGINE_GAME_VIEWPORT_OFFSET < 0 || config::VIEWPORT_WORLD_OFFSET < 0 {
                return null_mut();
            }

            let viewport = *(engine.cast::<u8>().add(config::ENGINE_GAME_VIEWPORT_OFFSET as usize) as *const *const u8);
            if viewport.is_null() {
                return null_mut();
            }

            *(viewport.add(config::VIEWPORT_WORLD_OFFSET as usize) as *const *mut UWorld)
        }
    }
}
