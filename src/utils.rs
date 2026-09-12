use std::ffi::c_void;
use std::fs::File;
use std::path::Path;

use grapnel::{Context, MidHook};
use windows::Win32::Foundation::{HMODULE, SYSTEMTIME};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Memory::{
    PAGE_EXECUTE_READWRITE, PAGE_PROTECTION_FLAGS, VirtualProtect,
};
use windows::Win32::System::ProcessStatus::{GetModuleInformation, MODULEINFO};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::Threading::GetCurrentProcess;

/// Formats a broken-down time: `YYYY-MM-DDTHH:MM:SS.mmm`
fn format_systemtime(t: &SYSTEMTIME) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond, t.wMilliseconds
    )
}

/// Installs a `fern` dispatch as the process-wide [`log`] sink, writing every
/// level into a freshly truncated file at `path`. Each record is emitted as
/// `[YYYY-MM-DDTHH:MM:SS.mmm] [LEVEL] message`.
///
/// The timestamp comes from `GetLocalTime` rather than a UTC clock so a line
/// lines up with the wall clock of whoever is reading the log next to the game
/// they just ran.
///
/// Dependencies log here too -- `grapnel` reports a hook it could not install
/// -- so a line from anything but this crate is tagged with its target. To
/// silence one without touching it, add
/// `.level_for("grapnel", log::LevelFilter::Off)` below; the level is a
/// runtime choice, so nothing needs recompiling to change it.
///
/// Both failures here are swallowed, because neither should stop a fix from
/// applying: the file not opening, and `apply` rejecting a second logger for
/// the process. The `log` macros are no-ops while no logger is installed, so
/// call sites need no guard of their own.
pub fn init_logging(path: &str) {
    let Ok(file) = File::create(path) else { return };
    let _ = fern::Dispatch::new()
        .format(|out, message, record| {
            let t = unsafe { GetLocalTime() };
            // A line from another crate carries its target, so grapnel
            // reporting a hook it could not install is not mistaken for this
            // mod's own reporting. This crate's lines stay untagged.
            if record.target().starts_with(env!("CARGO_CRATE_NAME")) {
                out.finish(format_args!(
                    "[{}] [{}] {message}",
                    format_systemtime(&t),
                    record.level()
                ))
            } else {
                out.finish(format_args!(
                    "[{}] [{}] [{}] {message}",
                    format_systemtime(&t),
                    record.level(),
                    record.target()
                ))
            }
        })
        .level(log::LevelFilter::Debug)
        .chain(file)
        .apply();
}

/// Logs a correction's report, if it made one. The C++ side returns an empty
/// string when a correction found nothing to change, or when its report repeats
/// the last one under the same key, which keeps a callback that runs every
/// frame from writing a line every frame.
pub fn log_report(report: String) {
    if !report.is_empty() {
        log::info!("{report}");
    }
}

/// A scanned/patched module: its base address plus everything worth logging
/// about the file backing it.
pub struct ModuleInfo {
    /// Base virtual address.
    pub address: HMODULE,
    /// Name of the module.
    pub name: String,
    /// Path of the filesystem to the exe.
    pub path: String,
    /// On-disk file size in bytes.
    pub size: u64,
}

impl ModuleInfo {
    /// Builds a `ModuleInfo` from a module handle, resolving its file name,
    /// size, and modified time for logging.
    pub fn new(address: HMODULE) -> Self {
        let mut buf = [0u16; 260];
        let len = unsafe { GetModuleFileNameW(Some(address), &mut buf) } as usize;
        let path = String::from_utf16_lossy(&buf[..len]);
        let name = Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        Self {
            address,
            name,
            path,
            size,
        }
    }
}

/// A mid-function hook keyed off a pattern scan.
pub struct SignatureHook {
    /// Name identifier for the hook.
    pub tag: &'static str,
    /// IDA-style byte string describing where the hook should be placed.
    pub signature: &'static str,
    /// Byte offset from the pattern match to the hook point.
    pub offset: usize,
}

/// Converts memory bytes into an IDA-style byte string, e.g. `[0x40, 0x63]` -> `"40 63"`.
pub fn bytes_to_string(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parses an IDA-style byte pattern (e.g. `"48 8B 05 ?? ?? ?? ?? 48 85 C0"`) into a
/// sequence of optional bytes, where `None` is a wildcard.
fn parse_pattern(pattern: &str) -> Vec<Option<u8>> {
    pattern
        .split_whitespace()
        .map(|byte| {
            if byte == "??" {
                None
            } else {
                Some(u8::from_str_radix(byte, 16).expect("invalid byte in AOB pattern"))
            }
        })
        .collect()
}

/// Overwrites memory at `address` with `pattern` (an IDA-style byte string,
/// e.g. `"DE AD BE EF"`), temporarily marking the region writable, then
/// restoring its original protection.
pub fn patch(address: usize, pattern: &str) {
    let bytes: Vec<u8> = pattern
        .split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).expect("invalid byte in patch pattern"))
        .collect();

    unsafe {
        let mut old_protect = PAGE_PROTECTION_FLAGS(0);
        let _ = VirtualProtect(
            address as *const c_void,
            bytes.len(),
            PAGE_EXECUTE_READWRITE,
            &mut old_protect,
        );
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), address as *mut u8, bytes.len());
        let mut discard = PAGE_PROTECTION_FLAGS(0);
        let _ = VirtualProtect(
            address as *const c_void,
            bytes.len(),
            old_protect,
            &mut discard,
        );
    }
}

/// Returns the base address and size of `module`'s loaded image.
fn get_module_range(module: HMODULE) -> Option<(*const u8, usize)> {
    unsafe {
        let process = GetCurrentProcess();
        let mut mod_info = MODULEINFO::default();
        GetModuleInformation(
            process,
            module,
            &mut mod_info,
            std::mem::size_of::<MODULEINFO>() as u32,
        )
        .ok()?;
        Some((
            mod_info.lpBaseOfDll as *const u8,
            mod_info.SizeOfImage as usize,
        ))
    }
}

/// Slides a window over `data` looking for the first match against `pattern`
/// (`None` entries are wildcards), returning the offset of the match.
fn aob_scan(data: &[u8], pattern: &[Option<u8>]) -> Option<usize> {
    if pattern.is_empty() || data.len() < pattern.len() {
        return None;
    }
    data.windows(pattern.len()).position(|window| {
        window
            .iter()
            .zip(pattern.iter())
            .all(|(byte, pat)| pat.is_none_or(|p| *byte == p))
    })
}

/// Scans `module`'s memory for `signature`, returning the absolute address of the
/// first match, if any.
pub fn pattern_scan(module: HMODULE, signature: &str) -> Option<usize> {
    let (base, size) = get_module_range(module)?;
    let data = unsafe { std::slice::from_raw_parts(base, size) };
    let pattern = parse_pattern(signature);
    aob_scan(data, &pattern).map(|offset| base as usize + offset)
}

/// Scans `module` for every occurrence of `signature`, returning all matching
/// absolute addresses.
///
/// Useful for data like UI strings, where the same text can appear more than
/// once and every copy needs patching.
pub fn pattern_scan_all(module: HMODULE, signature: &str) -> Vec<usize> {
    let Some((base, size)) = get_module_range(module) else {
        return Vec::new();
    };

    let data = unsafe { std::slice::from_raw_parts(base, size) };
    let pattern = parse_pattern(signature);
    let mut hits = Vec::new();
    if pattern.is_empty() {
        return hits;
    }

    let mut i = 0;
    while i + pattern.len() <= data.len() {
        let matched = data[i..i + pattern.len()]
            .iter()
            .zip(pattern.iter())
            .all(|(byte, pat)| pat.is_none_or(|p| *byte == p));
        if matched {
            hits.push(base as usize + i);
            i += pattern.len();
        } else {
            i += 1;
        }
    }
    hits
}

/// Installs a mid-function hook permanently at a fixed `offset` from
/// `module`'s base, bypassing signature scanning.
///
/// Useful for investigations that need to probe an address before it is worth
/// a signature at all. No shipped fix should call this: an absolute offset
/// moves whenever a game update shifts the code around it, so a confirmed hook
/// is converted to a signature before it ships. Kept, and deliberately not
/// removed as dead code, for the next investigation.
pub fn inject_hook_at<F>(module: &ModuleInfo, tag: &str, offset: usize, callback: F) -> bool
where
    F: FnMut(&mut Context) + Send + 'static,
{
    let hook_addr = (module.address.0 as usize + offset) as *mut u8;
    match MidHook::install(hook_addr, callback) {
        Ok(_hook) => {
            log::info!("{tag} : Hooked @ {}+{offset:x}", module.name);
            true
        }
        Err(e) => {
            log::error!(
                "{tag} : Failed to install hook @ {}+{offset:x}: {e}",
                module.name
            );
            false
        }
    }
}

/// Scans `module` for `sh.signature` and, if found, installs a mid-function
/// hook permanently for the duration of the main process at the address plus
/// `sh.offset` that jumps to the `callback`. Reports whether the hook went in.
pub fn inject_hook<F>(module: &ModuleInfo, sh: &SignatureHook, callback: F) -> bool
where
    F: FnMut(&mut Context) + Send + 'static,
{
    match pattern_scan(module.address, sh.signature) {
        Some(addr) => {
            let rel_addr = addr - module.address.0 as usize;
            log::info!(
                "{} : Found '{}' @ {}+{:x}",
                sh.tag,
                sh.signature,
                module.name,
                rel_addr
            );

            let hook_addr = (addr + sh.offset) as *mut u8;
            match MidHook::install(hook_addr, callback) {
                Ok(_hook) => {
                    log::info!(
                        "{} : Hooked @ {}+{:x}",
                        sh.tag,
                        module.name,
                        rel_addr + sh.offset
                    );
                    true
                }
                Err(e) => {
                    log::error!(
                        "{} : Failed to install hook for '{}': {e}",
                        sh.tag,
                        sh.signature
                    );
                    false
                }
            }
        }
        None => {
            log::error!("{} : Did not find '{}'", sh.tag, sh.signature);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{aob_scan, parse_pattern};
    #[test]
    fn reference_scanner_handles_wildcards_and_bounds() {
        let pattern = parse_pattern("40 ?? 57");
        assert_eq!(aob_scan(&[0, 0x40, 0x55, 0x57], &pattern), Some(1));
        assert_eq!(aob_scan(&[0x40, 0x55], &pattern), None);
        assert_eq!(aob_scan(&[0x40, 0x55, 0x58], &pattern), None);
        assert_eq!(aob_scan(&[0x40], &[]), None);
    }
}
