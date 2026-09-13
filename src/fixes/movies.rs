//! Everything that follows a playing movie: fitting its image to 16:9 and the
//! black backdrop wrapped behind it.

use crate::{
    bridge::ffi,
    utils::{ModuleInfo, SignatureHook, inject_hook, log_report},
};

/// Fits a playing movie to 16:9 from the native callbacks that own its image:
/// the movie screen's frame update and texture assignment, and the point where
/// creating a movie screen completes.
pub(super) fn install(module: &ModuleInfo) {
    let frame = SignatureHook {
        tag: "MovieScreen::OnFrame",
        signature: "40 53 48 83 EC 20 48 8B 99 68 03 00 00 48 8B 8B 68 03 00 00 E8 ?? ?? ?? ?? 48 85 C0 74 45",
        offset: 0,
    };
    inject_hook(module, &frame, |ctx| {
        log_report(unsafe { ffi::on_native_movie(ctx.rcx as usize, true) });
    });
    let texture = SignatureHook {
        tag: "MovieProjector::SetManaTexture",
        signature: "48 89 5C 24 10 57 48 83 EC 20 48 8B D9 48 8B FA 48 8B 89 68 03 00 00 E8 ?? ?? ?? ??",
        offset: 0,
    };
    inject_hook(module, &texture, |ctx| {
        // RDX is the incoming texture; a cleared one starts no playback.
        if ctx.rdx == 0 {
            return;
        }
        log_report(unsafe { ffi::on_native_movie(ctx.rcx as usize, false) });
    });
    let created = SignatureHook {
        tag: "CreateMovieScreen completion",
        signature: "33 D2 48 89 5F 30 48 8B CB E8 ?? ?? ?? ?? 48 8B 47 30 48 83 C4 20 5F 5E 5B C3",
        offset: 14,
    };
    inject_hook(module, &created, |ctx| {
        // RBX holds the screen the creation stored, null when it failed.
        if ctx.rbx == 0 {
            return;
        }
        log_report(unsafe { ffi::on_native_movie(ctx.rbx as usize, true) });
    });
    // A battle sequence dispatches its authored events through one native
    // function, tagged by the EventType byte at the head of the parameter
    // struct. The signature is that dispatch reading the tag and bounding it
    // against the last event type, which is unique image-wide with no
    // wildcards: MOVZX EAX,byte ptr [R8]; DEC EAX; CMP EAX,0x7A. Hooking the
    // read rather than the function entry keeps the anchor adjacent to the
    // field that is read, and R8 still holds the struct there.
    let sequence = SignatureHook {
        tag: "Battle sequence event",
        signature: "41 0F B6 00 FF C8 83 F8 7A 0F 87 ?? ?? ?? ??",
        offset: 0,
    };
    inject_hook(module, &sequence, |ctx| {
        log_report(unsafe { ffi::on_battle_sequence_event(ctx.r8 as usize) });
    });
}

/// Letterboxes a summon's movie image, from the shared render-transform hook
/// that is about to consume the scale field.
pub(super) fn on_transform(widget: usize) {
    log_report(unsafe { ffi::on_movie_transform(widget) });
}

/// Carries a wrapped movie root's visibility onto the wrapper that holds its
/// black backdrop, so the backdrop is dismissed with the movie.
pub(super) fn on_visibility(widget: usize, visibility: u8) {
    unsafe { ffi::sync_movie_backdrop(widget, visibility) };
}

/// Wraps a movie's widget-tree root in an overlay carrying a black backdrop,
/// from the rebuild that runs before the tree root is consumed.
pub(super) fn on_rebuild(widget: usize) {
    log_report(unsafe { ffi::wrap_movie_root(widget) });
}
