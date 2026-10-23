//! Strings and names: FString, FUtf8String, FAnsiString, FAllocatedString, FName, FText.

use core::ffi::c_void;
use core::fmt;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

use super::{GetImageBase, TArray};
use crate::config;

/// `FString`: UTF-16, null-terminated, the terminator is counted in `Num()`.
/// Derefs to `TArray<u16>` like the C++ class inherits from `TArray<wchar_t>`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FString(pub TArray<u16>);

impl FString {
    /// An FString over memory you own. The engine must not free or grow it.
    pub const fn from_raw(data: *mut u16, num: i32, max: i32) -> Self {
        Self(TArray::from_raw(data, num, max))
    }

    /// The UTF-16 code units without the terminator.
    pub fn as_utf16(&self) -> &[u16] {
        let s = self.0.as_slice();
        match s.iter().position(|&c| c == 0) {
            Some(end) => &s[..end],
            None => s,
        }
    }

    pub fn ToString(&self) -> String {
        char::decode_utf16(self.as_utf16().iter().copied()).map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER)).collect()
    }

    pub fn CStr(&self) -> *mut u16 {
        self.0.Data
    }
}

impl Deref for FString {
    type Target = TArray<u16>;
    fn deref(&self) -> &TArray<u16> {
        &self.0
    }
}

impl DerefMut for FString {
    fn deref_mut(&mut self) -> &mut TArray<u16> {
        &mut self.0
    }
}

/// Compares the text, like the C++ SDK.
impl PartialEq for FString {
    fn eq(&self, other: &Self) -> bool {
        self.as_utf16() == other.as_utf16()
    }
}

impl fmt::Display for FString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.ToString())
    }
}

impl fmt::Debug for FString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.ToString(), f)
    }
}

/// An `FString` backed by Rust-owned memory. The memory is freed when this is dropped.
/// - `FAllocatedString::from("text")`: a string to pass into the game.
/// - `FAllocatedString::new(Size)`: an empty buffer for functions that fill it without reallocating.
pub struct FAllocatedString {
    buffer: Vec<u16>,
}

impl FAllocatedString {
    pub fn new(Size: i32) -> Self {
        Self { buffer: Vec::with_capacity(Size.max(0) as usize) }
    }

    /// The `FString` view to pass to the game. Keep `self` alive while the game uses it.
    pub fn Get(&mut self) -> FString {
        FString::from_raw(self.buffer.as_mut_ptr(), self.buffer.len() as i32, self.buffer.capacity() as i32)
    }
}

impl From<&str> for FAllocatedString {
    fn from(text: &str) -> Self {
        let mut buffer: Vec<u16> = text.encode_utf16().collect();
        buffer.push(0);
        Self { buffer }
    }
}

/// Number of UTF-16 code units in `text` (used by `fstring!`).
#[doc(hidden)]
pub const fn utf16_len(text: &str) -> usize {
    let bytes = text.as_bytes();
    let (mut i, mut len) = (0, 0);
    while i < bytes.len() {
        let lead = bytes[i];
        let width = if lead < 0x80 { 1 } else if lead < 0xE0 { 2 } else if lead < 0xF0 { 3 } else { 4 };
        len += if width == 4 { 2 } else { 1 };
        i += width;
    }
    len
}

/// `text` as null-terminated UTF-16 (used by `fstring!`).
#[doc(hidden)]
pub const fn utf16_encode<const N: usize>(text: &str) -> [u16; N] {
    let bytes = text.as_bytes();
    let mut out = [0u16; N];
    let (mut i, mut o) = (0, 0);
    while i < bytes.len() {
        let lead = bytes[i] as u32;
        let (width, mut cp) = if lead < 0x80 { (1, lead) } else if lead < 0xE0 { (2, lead & 0x1F) } else if lead < 0xF0 { (3, lead & 0x0F) } else { (4, lead & 0x07) };
        let mut k = 1;
        while k < width {
            cp = (cp << 6) | (bytes[i + k] as u32 & 0x3F);
            k += 1;
        }
        if cp >= 0x10000 {
            let v = cp - 0x10000;
            out[o] = (0xD800 + (v >> 10)) as u16;
            out[o + 1] = (0xDC00 + (v & 0x3FF)) as u16;
            o += 2;
        } else {
            out[o] = cp as u16;
            o += 1;
        }
        i += width;
    }
    out
}

/// `FUtf8String`
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FUtf8String(pub TArray<u8>);

impl FUtf8String {
    pub fn ToString(&self) -> String {
        let s = self.0.as_slice();
        let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
        String::from_utf8_lossy(&s[..end]).into_owned()
    }

    pub fn CStr(&self) -> *mut u8 {
        self.0.Data
    }
}

impl Deref for FUtf8String {
    type Target = TArray<u8>;
    fn deref(&self) -> &TArray<u8> {
        &self.0
    }
}

impl PartialEq for FUtf8String {
    fn eq(&self, other: &Self) -> bool {
        self.ToString() == other.ToString()
    }
}

impl fmt::Display for FUtf8String {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.ToString())
    }
}

/// `FAnsiString`
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FAnsiString(pub TArray<u8>);

impl FAnsiString {
    pub fn ToString(&self) -> String {
        self.0.as_slice().iter().take_while(|&&c| c != 0).map(|&c| c as char).collect()
    }

    pub fn CStr(&self) -> *mut u8 {
        self.0.Data
    }
}

impl Deref for FAnsiString {
    type Target = TArray<u8>;
    fn deref(&self) -> &TArray<u8> {
        &self.0
    }
}

impl PartialEq for FAnsiString {
    fn eq(&self, other: &Self) -> bool {
        self.ToString() == other.ToString()
    }
}

impl fmt::Display for FAnsiString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.ToString())
    }
}

/// How this game turns an FName into a string (set in `config.rs`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NameMode {
    /// Call `FName::AppendString(FString&)`.
    AppendString,
    /// Call `GetNameEntryFromName(ComparisonIndex)`, then `AppendString(Entry, FString&)`.
    InlinedAppendString,
    /// Read the FNamePool directly.
    NamePool,
}

/// Address set with `FName::InitManually`, replacing the AppendString / GNames address from `config.rs`.
static NAME_OVERRIDE: AtomicUsize = AtomicUsize::new(0);

/// `FName`. Its size and field layout depend on the game (see `config.rs`).
#[repr(C, align(4))]
#[derive(Clone, Copy)]
pub struct FName {
    pub ComparisonIndex: i32,
    Rest: [u8; config::FNAME_SIZE - 4],
}

impl Default for FName {
    fn default() -> Self {
        Self { ComparisonIndex: 0, Rest: [0; config::FNAME_SIZE - 4] }
    }
}

impl FName {
    /// A name from its pool index and instance number (the number is dropped on games with outline numbers).
    pub fn new(ComparisonIndex: i32, Number: u32) -> Self {
        let mut name = Self { ComparisonIndex, ..Self::default() };
        name.write_i32(config::FNAME_NUMBER_OFFSET, Number as i32);
        name.write_i32(config::FNAME_DISPLAY_INDEX_OFFSET, ComparisonIndex);
        name
    }

    /// Converts a string to an FName through the engine (`UKismetStringLibrary::Conv_StringToName`).
    pub fn FromString(Name: &str) -> Self {
        let mut text = FAllocatedString::from(Name);
        crate::config::UKismetStringLibrary::Conv_StringToName(&text.Get())
    }

    /// Overrides the address of `AppendString` (or of GNames on games without it) found by Dumper-7.
    pub fn InitManually(Location: *mut c_void) {
        NAME_OVERRIDE.store(Location as usize, Ordering::Relaxed);
    }

    fn read_i32(&self, offset: isize) -> Option<i32> {
        if offset < 4 {
            return None;
        }
        let start = offset as usize - 4;
        let bytes = self.Rest.get(start..start + 4)?;
        Some(i32::from_le_bytes(bytes.try_into().ok()?))
    }

    fn write_i32(&mut self, offset: isize, value: i32) {
        if offset < 4 {
            return;
        }
        let start = offset as usize - 4;
        if let Some(bytes) = self.Rest.get_mut(start..start + 4) {
            bytes.copy_from_slice(&value.to_le_bytes());
        }
    }

    /// The instance number (`Foo_3` has number 4). Always 0 on games with outline numbers.
    pub fn GetNumber(&self) -> u32 {
        self.read_i32(config::FNAME_NUMBER_OFFSET).unwrap_or(0) as u32
    }

    /// The index used for display (differs from `ComparisonIndex` on case-preserving builds).
    pub fn GetDisplayIndex(&self) -> i32 {
        self.read_i32(config::FNAME_DISPLAY_INDEX_OFFSET).unwrap_or(self.ComparisonIndex)
    }

    pub fn IsNone(&self) -> bool {
        self.GetDisplayIndex() == 0 && self.GetNumber() == 0
    }

    /// The full name, including any path (e.g. `/Script/Engine.Actor`).
    pub fn GetRawString(&self) -> String {
        let overridden = NAME_OVERRIDE.load(Ordering::Relaxed);

        let base = unsafe {
            match config::NAME_MODE {
                NameMode::AppendString => {
                    let append_string = if overridden != 0 { overridden } else { GetImageBase() + config::APPEND_STRING };
                    return append_string_to(append_string, (self as *const Self).cast());
                }
                NameMode::InlinedAppendString => {
                    type GetNameEntryFn = unsafe extern "C" fn(u32) -> *const c_void;
                    let get_entry: GetNameEntryFn = core::mem::transmute(GetImageBase() + config::GET_NAME_ENTRY);
                    let entry = config::call_game_function!(get_entry, self.ComparisonIndex as u32);

                    let append_string = if overridden != 0 { overridden } else { GetImageBase() + config::APPEND_STRING };
                    append_string_to(append_string, entry)
                }
                NameMode::NamePool => {
                    let pool = if overridden != 0 { overridden } else { GetImageBase() + config::GNAMES };
                    name_pool_string(pool as *const u8, self.GetDisplayIndex()).unwrap_or_default()
                }
            }
        };

        match self.GetNumber() {
            0 => base,
            n => format!("{}_{}", base, n - 1),
        }
    }

    /// The name without its path, e.g. `Actor`.
    pub fn ToString(&self) -> String {
        let raw = self.GetRawString();
        match raw.rfind('/') {
            Some(pos) => raw[pos + 1..].to_owned(),
            None => raw,
        }
    }
}

impl PartialEq for FName {
    fn eq(&self, other: &Self) -> bool {
        self.ComparisonIndex == other.ComparisonIndex && self.GetNumber() == other.GetNumber()
    }
}
impl Eq for FName {}

impl fmt::Display for FName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.ToString())
    }
}

impl fmt::Debug for FName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.GetRawString(), f)
    }
}

/// Converts a name once through the engine and caches it (used by `StaticName()`).
#[doc(hidden)]
pub struct NameCache {
    name: &'static str,
    value: OnceLock<FName>,
}

impl NameCache {
    pub const fn new(name: &'static str) -> Self {
        Self { name, value: OnceLock::new() }
    }

    /// Retries until the engine returns a valid name, like the C++ SDK.
    pub fn get(&self) -> FName {
        if let Some(name) = self.value.get() {
            return *name;
        }

        let name = FName::FromString(self.name);
        if !name.IsNone() {
            let _ = self.value.set(name);
        }
        name
    }
}

/// Calls the game's `AppendString(const void* NameOrEntry, FString& Out)` into a stack buffer.
unsafe fn append_string_to(append_string: usize, name_or_entry: *const c_void) -> String {
    type AppendStringFn = unsafe extern "C" fn(*const c_void, *mut FString);

    let mut buffer = [0u16; 1024];
    let mut out = FString::from_raw(buffer.as_mut_ptr(), 0, buffer.len() as i32);

    unsafe {
        let append: AppendStringFn = core::mem::transmute(append_string);
        config::call_game_function!(append, name_or_entry, &mut out);
    }

    out.ToString()
}

/// Reads an entry of the FNamePool directly (used when AppendString couldn't be found).
unsafe fn name_pool_string(pool: *const u8, index: i32) -> Option<String> {
    let block = (index >> config::NAMEPOOL_BLOCK_OFFSET_BITS) as u32;
    let offset = (index & ((1 << config::NAMEPOOL_BLOCK_OFFSET_BITS) - 1)) as usize * config::NAMEPOOL_ENTRY_STRIDE;

    unsafe {
        let current_block = *(pool.add(config::NAMEPOOL_CURRENT_BLOCK) as *const u32);
        let current_cursor = *(pool.add(config::NAMEPOOL_CURRENT_BYTE_CURSOR) as *const u32) as usize;

        if index < 0 || block > current_block || (block == current_block && offset > current_cursor) {
            return None;
        }

        let blocks = pool.add(config::NAMEPOOL_BLOCKS) as *const *const u8;
        let block_ptr = *blocks.add(block as usize);
        if block_ptr.is_null() {
            return None;
        }

        let entry = block_ptr.add(offset);
        let header = (entry.add(config::NAMEENTRY_HEADER_OFFSET) as *const u16).read_unaligned();
        let string = entry.add(config::NAMEENTRY_STRING_OFFSET);

        let (len_shift, len_mask) = if config::CASE_PRESERVING_NAME { (1, 0x7FFF) } else { (6, 0x3FF) };
        let is_wide = header & 1 != 0;
        let len = ((header >> len_shift) & len_mask) as usize;

        // Outline-number games store "Name_3" as an entry with length 0 that points at "Name".
        if config::OUTLINE_NUMBER_NAME && len == 0 {
            let numbered = string.add(if config::CASE_PRESERVING_NAME { 2 } else { 0 });
            let id = (numbered as *const u32).read_unaligned();
            let number = (numbered.add(4) as *const u32).read_unaligned();

            let base = name_pool_string(pool, id as i32)?;
            return Some(if number > 0 { format!("{}_{}", base, number - 1) } else { base });
        }

        if is_wide {
            let chars = (0..len).map(|i| (string as *const u16).add(i).read_unaligned());
            Some(char::decode_utf16(chars).map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER)).collect())
        } else {
            Some((0..len).map(|i| *string.add(i) as char).collect())
        }
    }
}

/// `FText`. Only reading the display string is supported, like the C++ SDK.
#[repr(C, align(8))]
#[derive(Clone, Copy)]
pub struct FText {
    pub Data: [u8; config::TEXT_SIZE],
}

impl FText {
    /// The text's source string, if it has one.
    pub fn GetStringRef(&self) -> Option<&FString> {
        unsafe {
            let text_data = (self.Data.as_ptr().add(config::TEXT_DATA_OFFSET) as *const *const u8).read_unaligned();
            if text_data.is_null() {
                return None;
            }
            Some(&*(text_data.add(config::TEXT_DATA_STRING_OFFSET) as *const FString))
        }
    }

    pub fn ToString(&self) -> String {
        self.GetStringRef().map(FString::ToString).unwrap_or_default()
    }
}

impl fmt::Display for FText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.ToString())
    }
}
