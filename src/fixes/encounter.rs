//! The encounter transition's screen capture, re-framed to the shape it is
//! drawn at.

use crate::{
    bridge::ffi,
    utils::{ModuleInfo, SignatureHook, inject_hook, log_report},
};

/// Remembers the render target `CPP_BackBufferCaptureSubsystem::Capture` is
/// handed, and that a fresh frame is on its way into it.
///
/// The whole function is two writes -- the target at +0x28 and the request word
/// at +0x30 -- and the signature matches them, unique image-wide with no
/// wildcards. Nothing is written to the game from here: the copy the request
/// raises is deferred, so the frame is re-framed afterwards, from the widget
/// that draws it.
pub(super) fn install(module: &ModuleInfo) {
    let capture = SignatureHook {
        tag: "Back buffer capture",
        signature: "48 8B 41 30 48 85 C0 74 0A 48 89 50 28 66 C7 40 30 00 01 C3",
        offset: 0,
    };
    inject_hook(module, &capture, |ctx| {
        // RCX is the subsystem, RDX the render target it was handed.
        log_report(unsafe { ffi::note_capture_target(ctx.rcx as usize, ctx.rdx as usize) });
    });
}
