//! Values Dumper-7 found for FFRS (5.6.1-0+UE5).
//!
//! `image_base` and `decrypt_objects` may be edited, everything else is used by `rt` as-is.

use crate::rt;

pub const GAME_NAME: &str = "FFRS";
pub const GAME_VERSION: &str = "5.6.1-0+UE5";


// GObjects (FChunkedFixedUObjectArray)
pub const GOBJECTS: usize = 0x9D84DD0;
pub const GOBJECTS_OBJECTS: usize = 0x0;
pub const GOBJECTS_NUM_ELEMENTS: usize = 0x14;
pub const GOBJECTS_NUM_CHUNKS: usize = 0x1C;
pub const GOBJECTS_CHUNK_SIZE: i32 = 0x10000;
pub const GOBJECTS_ITEM_SIZE: usize = 0x18;
pub const GOBJECTS_ITEM_OBJECT_OFFSET: usize = 0x0;

// FName
pub const FNAME_SIZE: usize = 0x8;
pub const FNAME_NUMBER_OFFSET: isize = 4;
pub const FNAME_DISPLAY_INDEX_OFFSET: isize = -1;
pub const NAME_MODE: rt::NameMode = rt::NameMode::AppendString;
pub const APPEND_STRING: usize = 0x13763A0;
pub const GET_NAME_ENTRY: usize = 0x0;
pub const GNAMES: usize = 0x9C686F8;
pub const CASE_PRESERVING_NAME: bool = false;
pub const OUTLINE_NUMBER_NAME: bool = false;
pub const NAMEPOOL_BLOCK_OFFSET_BITS: i32 = 0x0;
pub const NAMEPOOL_ENTRY_STRIDE: usize = 0x0;
pub const NAMEPOOL_CURRENT_BLOCK: usize = 0x0;
pub const NAMEPOOL_CURRENT_BYTE_CURSOR: usize = 0x0;
pub const NAMEPOOL_BLOCKS: usize = 0x0;
pub const NAMEENTRY_HEADER_OFFSET: usize = 0x0;
pub const NAMEENTRY_STRING_OFFSET: usize = 0x0;

// Functions and globals
pub const PROCESS_EVENT_INDEX: usize = 0x4C;
pub const GWORLD: usize = 0x9AA0540;
pub const ENGINE_GAME_VIEWPORT_OFFSET: isize = 3088;
pub const VIEWPORT_WORLD_OFFSET: isize = 120;

// Sizes of engine types
pub const TEXT_SIZE: usize = 0x10;
pub const TEXT_DATA_OFFSET: usize = 0x0;
pub const TEXT_DATA_STRING_OFFSET: usize = 0x20;
pub const DELEGATE_SIZE: usize = 0x10;
pub const MULTICAST_INLINE_DELEGATE_SIZE: usize = 0x10;
pub const WEAK_POINTER_HAS_TAG: bool = false;

// Types the runtime uses
pub type UObject = crate::CoreUObject::UObject;
pub type UField = crate::CoreUObject::UField;
pub type UStruct = crate::CoreUObject::UStruct;
pub type UClass = crate::CoreUObject::UClass;
pub type UFunction = crate::CoreUObject::UFunction;
pub type UEngine = crate::Engine::UEngine;
pub type UWorld = crate::Engine::UWorld;
pub type FSoftObjectPath = crate::CoreUObject::FSoftObjectPath;

/// Base address of the game module.
pub fn image_base() -> usize {
    #[link(name = "kernel32")]
    unsafe extern "C" {
        fn GetModuleHandleW(module_name: *const u16) -> *mut ::core::ffi::c_void;
    }

    unsafe { GetModuleHandleW(::core::ptr::null()) as usize }
}

/// Decrypts the pointer to the GObjects chunk table (most games don't encrypt it).
pub fn decrypt_objects(objects: *mut u8) -> *mut u8 {
    objects
}

/// Offset of `UObject::ProcessEvent` from the image base (for hooking).
pub const PROCESS_EVENT: usize = 0x1592E30;

// More types the runtime uses
pub type UGameEngine = crate::Engine::UGameEngine;
pub type UKismetStringLibrary = crate::Engine::UKismetStringLibrary;
pub type FVector = crate::CoreUObject::FVector;
pub type FVector2D = crate::CoreUObject::FVector2D;
pub type FRotator = crate::CoreUObject::FRotator;

/// Every call into game code goes through this macro (like `InSDKUtils::CallGameFunction` in the C++ SDK).
/// Replace the body to route calls through your own trampoline.
macro_rules! call_game_function {
    ($function:expr $(, $argument:expr)* $(,)?) => {
        $function($($argument),*)
    };
}
pub(crate) use call_game_function;
