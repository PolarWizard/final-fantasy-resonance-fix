//! Calls into game code (the C++ SDK's `InSDKUtils`).
//!
//! The game is 64-bit MSVC code, so every game function is bound as `extern "C"`
//! (on x86_64-pc-windows-msvc that is the same ABI as `"system"`). Two MSVC rules to keep
//! in mind when binding anything new:
//! - Never pass a C++ class by value; pass a pointer.
//! - A C++ *member* function that returns a struct (of any size) returns it through a
//!   hidden pointer placed after `this`: bind it as `fn(this, *mut Ret) -> *mut Ret`.
//!
//! Every call goes through `config::call_game_function!`, which can be edited per game.

use core::ffi::c_void;
use core::ptr::null_mut;
use core::sync::atomic::{AtomicPtr, Ordering};

use super::{default_object, EFunctionFlags, FindFunctionInClass};
use crate::config::{self, UClass, UFunction, UObject};

/// Base address of the game module (see `config::image_base`).
#[inline]
pub fn GetImageBase() -> usize {
    config::image_base()
}

/// Reads entry `Index` of an object's vtable.
///
/// # Safety
/// `ObjectInstance` must point to a live C++ object with at least `Index + 1` vtable entries.
#[inline]
pub unsafe fn GetVirtualFunction(ObjectInstance: *const c_void, Index: usize) -> *const c_void {
    unsafe {
        let vtable = *(ObjectInstance as *const *const *const c_void);
        *vtable.add(Index)
    }
}

/// `void UObject::ProcessEvent(UFunction* Function, void* Parms)`
pub type ProcessEventFn = unsafe extern "C" fn(*mut UObject, *mut UFunction, *mut c_void);

/// `UFunction::ExecFunction`, the native thunk behind a UFunction (`void (*)(void* Context, void* TheStack, void* Result)`).
pub type FNativeFuncPtr = Option<unsafe extern "C" fn(Context: *mut c_void, TheStack: *mut c_void, Result: *mut c_void)>;

/// Calls `UObject::ProcessEvent` through the object's vtable.
///
/// # Safety
/// `obj` and `func` must be live, and `parms` must point to the function's parameter struct
/// (or be null if it has none). Call it on the game thread.
pub unsafe fn process_event(obj: *mut UObject, func: *mut UFunction, parms: *mut c_void) {
    unsafe {
        let process_event: ProcessEventFn = core::mem::transmute(GetVirtualFunction(obj.cast(), config::PROCESS_EVENT_INDEX));
        config::call_game_function!(process_event, obj, func, parms)
    }
}

/// Caches the UFunction behind one generated wrapper.
#[doc(hidden)]
pub struct FuncCache {
    class_name: &'static str,
    func_name: &'static str,
    func: AtomicPtr<UFunction>,
}

impl FuncCache {
    pub const fn new(class_name: &'static str, func_name: &'static str) -> Self {
        Self { class_name, func_name, func: AtomicPtr::new(null_mut()) }
    }

    /// Finds the function on `class` (or its supers) the first time, then returns the cached pointer.
    ///
    /// # Safety
    /// `class` must be null or a live UClass.
    pub unsafe fn get(&self, class: *mut UClass) -> *mut UFunction {
        let cached = self.func.load(Ordering::Relaxed);
        if !cached.is_null() {
            return cached;
        }

        let found = unsafe { FindFunctionInClass(class, self.class_name, self.func_name) };
        if !found.is_null() {
            self.func.store(found, Ordering::Relaxed);
        }
        found
    }
}

/// Calls a UFunction on `obj` (used by generated wrappers). Does nothing if `obj` is null or the
/// function can't be found, in which case the parameter struct (and so the return value) stays zeroed.
///
/// # Safety
/// See [`process_event`].
#[doc(hidden)]
pub unsafe fn call_ufunction(obj: *mut UObject, cache: &FuncCache, parms: *mut c_void, native: bool) {
    if obj.is_null() {
        return;
    }

    unsafe { call(obj, cache.get((*obj).Class), parms, native) };
}

/// Calls a static UFunction through the class default object of `class` (used by generated wrappers).
///
/// # Safety
/// See [`process_event`].
#[doc(hidden)]
pub unsafe fn call_static(class: *mut UClass, cache: &FuncCache, parms: *mut c_void, native: bool) {
    let obj = default_object(class);
    if obj.is_null() {
        return;
    }

    unsafe { call(obj, cache.get(class), parms, native) };
}

unsafe fn call(obj: *mut UObject, func: *mut UFunction, parms: *mut c_void, native: bool) {
    if func.is_null() {
        return;
    }

    unsafe {
        if !native {
            process_event(obj, func, parms);
            return;
        }

        // Same as the C++ SDK: mark native functions as native for the duration of the call.
        let old_flags = (*func).FunctionFlags;
        (*func).FunctionFlags = old_flags | EFunctionFlags::Native;
        process_event(obj, func, parms);
        (*func).FunctionFlags = old_flags;
    }
}
