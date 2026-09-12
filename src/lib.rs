mod bridge;
mod config;
mod fixes;
#[allow(dead_code)] // Shared reference utilities also serve mods with memory patches.
mod utils;

use std::ffi::c_void;

use std::panic::catch_unwind;

use utils::{ModuleInfo, init_logging};
use windows::Win32::Foundation::{CloseHandle, HMODULE};
use windows::Win32::System::LibraryLoader::GetModuleHandleA;
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
use windows::Win32::System::Threading::{
    CreateThread, SetThreadPriority, THREAD_CREATION_FLAGS, THREAD_PRIORITY_HIGHEST,
};

fn log_open(module: &ModuleInfo, this_module: HMODULE) {
    let path = config::module_dir(this_module)
        .expect("failed to locate mod directory")
        .join("final_fantasy_resonance_fix.log");
    init_logging(&path.to_string_lossy());

    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    log::info!("-------------------------------------");
    log::info!(
        "Version: {}-{} ({})",
        env!("CARGO_PKG_VERSION"),
        env!("GIT_HASH"),
        profile
    );
    log::info!("Rustc: {}", env!("RUSTC_VERSION"));
    log::info!("Module Name: {}", module.name);
    log::info!("Module Path: {}", module.path);
    log::info!("Module Addr: {:#x}", module.address.0 as usize);
    log::info!("Module Size: {:#x} ({} bytes)", module.size, module.size);
    log::info!("-------------------------------------");
}

/// True entry point of the DLL.
///
/// Applies all fixes and features of this mod to the game process it was
/// attached to based on the `Config` using `this_module` to determine
/// where the path on the filesystem where the exe is stored.
///
/// Panics are isolated and captured so if one is thrown it does not crash
/// the process and it is logged for debugging.
fn main(this_module: HMODULE) {
    // Catch a panic's message and location reach the log file rather than vanishing.
    std::panic::set_hook(Box::new(|info| log::error!("panic: {info}")));

    // Catches a panic here instead of letting it crash the process.
    let result = catch_unwind(|| {
        let handle = unsafe { GetModuleHandleA(None) }.expect("failed to get module handle");
        let module = ModuleInfo::new(handle);

        log_open(&module, this_module);

        let config = config::load(this_module);
        if !config.super_enable {
            log::info!("main: super_enable is false in the config, mod not applied");
            return;
        }

        fixes::install(&module, &config);
    });

    if result.is_err() {
        log::error!("main: panicked -- see the panic line above in this log for details");
    }
}

/// Windows ABI compatible entry point.
///
/// Because this is provided to the `CreateThread` Windows API function this
/// function needs to compiled to use the appropriate Windows ABI. Afterwards
/// `main`` can be called safely using the Rust ABI.
unsafe extern "system" fn windows_main(param: *mut c_void) -> u32 {
    main(HMODULE(param));
    0
}

/// Entry point of the DLL.
#[unsafe(no_mangle)]
extern "system" fn DllMain(hinst: HMODULE, reason: u32, _reserved: *mut c_void) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        unsafe {
            if let Ok(thread) = CreateThread(
                None,
                0,
                Some(windows_main),
                Some(hinst.0 as *const c_void),
                THREAD_CREATION_FLAGS(0),
                None,
            ) {
                let _ = SetThreadPriority(thread, THREAD_PRIORITY_HIGHEST);
                let _ = CloseHandle(thread);
            }
        }
    }
    1
}
