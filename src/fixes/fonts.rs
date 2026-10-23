//! Supplies a loose font to Unreal's existing FreeType loader.
//!
//! Both memory and streamed faces are opened through native FT_Open_Args.
//! The mod owns the replacement bytes until process exit; FreeType borrows
//! them. Original Unreal font objects, text sizes, and styles stay in place.
//!
//! # How the native loading boundary was found
//!
//! The SDK exposes font assets, but not the native constructors and ownership
//! needed to populate them from loose bytes. The executable also has no usable
//! `FT_New_Memory_Face` / `FT_Open_Face` symbol names. Discovery therefore used
//! FreeType's `src/base/ftobjs.c` control flow as a guide, then checked the local
//! machine code and its Slate callers rather than assuming a source version.
//!
//! Searching small functions for a null-buffer check, error 6, and a stack
//! store of `FT_OPEN_MEMORY` (1) identified the memory wrapper. Its caller in
//! Slate supplies the font buffer and size. A second wrapper reaches the same
//! internal loader from Slate's streamed-font path with `FT_OPEN_STREAM` (2).
//! Both wrappers must be hooked to cover these two ways of loading a face.
//! The signature comments in [`install`] describe the distinguishing bytes.
//!
//! The memory wrapper's AOB (array-of-bytes) signature matches once; the stream
//! wrapper is the wrapper of its shape that calls the same internal loader.
//! RVAs below record that investigation; runtime addresses come from signature
//! scans and decoded relative calls. Wildcards cover stack offsets and
//! branch/call displacements, not changes to instructions or ABI. This evidence
//! identifies the hook sites; it does not establish in-game rendering quality or
//! coverage of faces that were already cached before installation.

use std::{
    cell::Cell,
    fs::File,
    io::{self, Read},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use grapnel::Context;

use crate::{
    config::Fonts,
    utils::{ModuleInfo, inject_hook_at, pattern_scan, pattern_scan_all},
};

// FreeType's public FT_Open_Args ABI on Windows x64. FT_Long is 32 bits here.
// The memory wrapper constructs exactly these fields in its stack local.
#[repr(C)]
#[derive(Default)]
struct OpenArgs {
    flags: u32,
    memory_base: usize,
    memory_size: i32,
    pathname: usize,
    stream: usize,
    driver: usize,
    num_params: i32,
    params: usize,
}

const _: () = {
    assert!(std::mem::size_of::<OpenArgs>() == 0x40);
    assert!(std::mem::offset_of!(OpenArgs, memory_base) == 0x08);
    assert!(std::mem::offset_of!(OpenArgs, memory_size) == 0x10);
    assert!(std::mem::offset_of!(OpenArgs, stream) == 0x20);
};

/// Internal loader ABI: library, arguments, face index, output face, test_mac.
/// Both public wrappers supply 1 for the fifth argument on the stack.
type OpenFace = unsafe extern "C" fn(usize, *const OpenArgs, i32, *mut usize, u8) -> i32;

// Windows FT_StreamRec: unsigned long size/pos are 32 bits. When we replace
// a streamed source with memory, FreeType never receives the original stream
// and cannot invoke its close callback. Close that unused source on success.
#[repr(C)]
#[derive(Default)]
struct FontStream {
    base: usize,
    size: u32,
    pos: u32,
    descriptor: usize,
    pathname: usize,
    read: usize,
    close: Option<unsafe extern "C" fn(*mut FontStream)>,
    memory: usize,
    cursor: usize,
    limit: usize,
}

const _: () = assert!(std::mem::offset_of!(FontStream, close) == 0x28);

/// Releases an original streamed source after a replacement face opens.
///
/// FreeType only saw our memory arguments, so it cannot close that source.
/// The inspected Unreal stream callback deletes its file reader and clears
/// the reader pointer; the owning object's destructor relies on this cleanup.
/// On rejection, the original loader retry takes responsibility instead.
///
/// # Safety
///
/// `args` must point to the still-live original arguments. If FT_OPEN_STREAM is
/// set, its non-null stream and optional close callback must remain valid and
/// must not have been closed already. Unused argument fields may be uninitialized.
unsafe fn close_unused_stream(args: *const OpenArgs) {
    // FT_New_Memory_Face initializes only the fields its flags select. Read
    // these members individually without copying uninitialized unused fields.
    let flags = unsafe { std::ptr::addr_of!((*args).flags).read() };
    if flags & 2 == 0 {
        return;
    }
    let stream = unsafe { std::ptr::addr_of!((*args).stream).read() } as *mut FontStream;
    if !stream.is_null() {
        if let Some(close) = unsafe { std::ptr::addr_of!((*stream).close).read() } {
            unsafe { close(stream) };
        }
    }
}

struct Replacement {
    // Kept with args so every face opened from memory keeps valid backing data.
    _bytes: Box<[u8]>,
    args: OpenArgs,
    reported: AtomicBool,
    rejected: AtomicBool,
}

impl Replacement {
    /// Owns the font bytes at a stable address and builds borrowed native args.
    ///
    /// `bytes` must have the positive, i32-sized length checked by [`read_font`].
    /// Hook closures retain this object through an Arc for process lifetime;
    /// FreeType does not copy or take ownership of the Rust allocation.
    fn new(bytes: Vec<u8>) -> Self {
        let bytes = bytes.into_boxed_slice();
        let args = OpenArgs {
            flags: 1, // FT_OPEN_MEMORY
            memory_base: bytes.as_ptr() as usize,
            memory_size: bytes.len() as i32,
            ..OpenArgs::default()
        };
        Self {
            _bytes: bytes,
            args,
            reported: AtomicBool::new(false),
            rejected: AtomicBool::new(false),
        }
    }
}

#[derive(Clone, Copy)]
struct Pending {
    stack: u64,
    library: usize,
    args: usize,
    face_index: i32,
    output: usize,
}

thread_local! {
    // Nested FreeType calls use their own inputs. Match the wrapper's stack
    // address on return so an inner call cannot consume an outer request.
    static PENDING: Cell<Option<Pending>> = const { Cell::new(None) };
}

/// Substitutes arguments immediately before the wrapper's internal CALL.
///
/// Windows x64 passes library in RCX, OpenArgs in RDX, face index in R8D, and
/// the output-face pointer in R9. Save them for fallback and select face zero
/// from the loose font. Negative indices query face counts and pass through.
/// A pending request also blocks nested substitution on this thread; its RSP
/// lets [`finish`] distinguish the outer return from an inner wrapper return.
fn begin(ctx: &mut Context, replacement: &Replacement) {
    if replacement.rejected.load(Ordering::Relaxed)
        || PENDING.get().is_some()
        || (ctx.r8 as i32) < 0 // Preserve FreeType's face-count probes.
        || ctx.rcx == 0
        || ctx.rdx == 0
        || ctx.r9 == 0
    {
        return;
    }
    PENDING.set(Some(Pending {
        stack: ctx.rsp,
        library: ctx.rcx as usize,
        args: ctx.rdx as usize,
        face_index: ctx.r8 as i32,
        output: ctx.r9 as usize,
    }));
    ctx.rdx = &replacement.args as *const OpenArgs as u64;
    ctx.r8 = 0; // Single TTF/OTF face; discard the original font's subface index.
}

/// Handles the internal loader's result before the wrapper's epilogue runs.
///
/// CALL/RET restores the pre-call RSP, so only the matching pending request is
/// consumed. EAX holds the FreeType error code. Success closes an unused source
/// stream; rejection disables further substitutions and retries the unhooked
/// loader with the saved original arguments. Its result replaces EAX so Unreal
/// observes the retry's outcome. Font acceptance is logged once, not per face.
fn finish(ctx: &mut Context, replacement: &Replacement, original: OpenFace) {
    let Some(pending) = PENDING.get().filter(|pending| pending.stack == ctx.rsp) else {
        return;
    };
    PENDING.set(None);
    let error = ctx.rax as i32;
    if error == 0 {
        unsafe { close_unused_stream(pending.args as *const OpenArgs) };
        if !replacement.reported.swap(true, Ordering::Relaxed) {
            log::info!("Fonts: FreeType accepted the custom font; first replacement face opened");
        }
        return;
    }

    // Disable substitution before retrying; this also bypasses any nested opens
    // from the retry, so a separate thread-local retry flag is unnecessary.
    let first_rejection = !replacement.rejected.swap(true, Ordering::Relaxed);
    // This is the unhooked internal loader. Both wrappers pass test_mac=1.
    // The original arguments remain alive on the wrapper's stack/in its owner.
    let result = unsafe {
        original(
            pending.library,
            pending.args as *const OpenArgs,
            pending.face_index,
            pending.output as *mut usize,
            1,
        )
    };
    ctx.rax = result as u32 as u64;
    if first_rejection {
        log::warn!(
            "Fonts: custom font rejected (FreeType error {error}); original font retry returned \
             {result}. Further replacements disabled for this session"
        );
    }
}

/// The target of the five-byte E8 CALL at `call`: its signed rel32 operand
/// decoded as `call + 5 + displacement`.
fn call_target(call: usize) -> usize {
    let displacement = unsafe { ((call + 1) as *const i32).read_unaligned() };
    (call + 5).wrapping_add_signed(displacement as isize)
}

/// Installs the argument/result hooks around the native E8 CALL at `call`,
/// found by signature, whose target is the callable fallback loader.
///
/// The result hook sits at CALL+5 and must install before argument substitution
/// is allowed. Captured Arcs retain the buffer even if only one hook installs.
fn install_pair(
    module: &ModuleInfo,
    replacement: Arc<Replacement>,
    call: usize,
    tags: [&str; 2],
) -> bool {
    let original: OpenFace = unsafe { std::mem::transmute(call_target(call)) };
    let offset = call - module.address.0 as usize;
    let result_data = Arc::clone(&replacement);
    if !inject_hook_at(module, tags[1], offset + 5, move |ctx| {
        finish(ctx, &result_data, original);
    }) {
        return false;
    }
    // Shared MidHooks live until process exit, retaining this Arc and its font
    // buffer even after install() returns or another path fails to install.
    inject_hook_at(module, tags[0], offset, move |ctx| {
        begin(ctx, &replacement);
    })
}

/// Reads a font once, bounded by its initial size and Windows' 32-bit FT_Long.
///
/// Rejects empty/oversized files, allocation failures, read errors, and a short
/// read if the file shrinks. Format validation is left to the game's FreeType
/// loader, which can reject the replacement and trigger the original-font retry.
fn read_font(path: &Path) -> io::Result<Vec<u8>> {
    let file = File::open(path)?;
    let size = file.metadata()?.len();
    if size == 0 || size > i32::MAX as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "font size must fit a positive FT_Long",
        ));
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size as usize)
        .map_err(io::Error::other)?;
    // Bound the read even if the file grows while it is being opened.
    file.take(size).read_to_end(&mut bytes)?;
    if bytes.len() as u64 != size {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "font changed while reading",
        ));
    }
    Ok(bytes)
}

/// Loads the font found beside the ASI and hooks both memory and streamed faces.
///
/// Config loading has already picked the font. Disabled settings, no font, or
/// file-read failures skip installation. Each path needs its own hook pair;
/// partial installation is logged because it leaves some font loads unchanged.
pub(super) fn install(module: &ModuleInfo, config: &Fonts) {
    if !config.enable {
        return;
    }
    let Some(path) = &config.file else {
        log::info!("Fonts: no .ttf or .otf in the scripts folder, keeping the game's font");
        return;
    };
    let bytes = match read_font(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            log::error!("Fonts: could not read {} ({error})", path.display());
            return;
        }
    };
    log::info!("Fonts: loaded {} ({} bytes)", path.display(), bytes.len());
    let replacement = Arc::new(Replacement::new(bytes));
    // FT_New_Memory_Face: discovered by its `test rdx, rdx`, null-input error
    // `mov eax, 6`, and stack-built FT_Open_Args. The signature keeps the
    // stores that build them -- buffer, size, flags=1 (FT_OPEN_MEMORY), null
    // stream -- with their stack offsets wildcarded. Slate's call at RVA
    // 0x1691632 supplies these memory inputs, confirming this is a
    // font-loading path.
    //
    // The wrapper is at RVA 0x6DD5CE0; +0x46 is its E8 internal CALL, made
    // after the wrapper has rearranged arguments into the common OpenFace ABI,
    // and the result hook goes on the instruction after it.
    const MEMORY: &str = "48 83 EC ?? 41 8B C1 48 85 D2 75 ?? B8 06 00 00 00 48 83 C4 ?? C3 4C 8B 8C 24 ?? ?? ?? ?? 48 89 54 24 ?? 48 8D 54 24 ?? 44 89 44 24 ?? 44 8B C0 C7 44 24 ?? 01 00 00 00 48 C7 44 24 ?? 00 00 00 00 C6 44 24 20 01 E8 ?? ?? ?? ??";
    let Some(memory_wrapper) = pattern_scan(module.address, MEMORY) else {
        log::error!("Fonts: memory wrapper signature not found");
        return;
    };
    let memory_call = memory_wrapper + 0x46;
    // FT_Open_Face: Slate's calls at RVA 0x1680203 and 0x6E0B3BD pass flags=2
    // and a stream. This wrapper (RVA 0x6DD5F00) only reserves stack, sets
    // test_mac=1 at [rsp+0x20] and calls the internal loader (RVA 0x6DDBE90).
    // Another wrapper of exactly that shape exists (RVA 0x215EE90), so of
    // every match the one calling the same loader as the memory wrapper is
    // taken; +9 is its CALL.
    const STREAM: &str = "48 83 EC ?? C6 44 24 20 01 E8 ?? ?? ?? ?? 48 83 C4 ?? C3";
    let loader = call_target(memory_call);
    let stream_call = pattern_scan_all(module.address, STREAM)
        .into_iter()
        .map(|wrapper| wrapper + 9)
        .find(|&call| call_target(call) == loader);
    let memory = install_pair(
        module,
        Arc::clone(&replacement),
        memory_call,
        ["Font memory input", "Font memory result"],
    );
    let stream = match stream_call {
        Some(call) => install_pair(
            module,
            replacement,
            call,
            ["Font stream input", "Font stream result"],
        ),
        None => {
            log::error!("Fonts: stream wrapper not found");
            false
        }
    };
    if !memory || !stream {
        log::warn!("Fonts: incomplete hook coverage (memory={memory}, stream={stream})");
    }
}
