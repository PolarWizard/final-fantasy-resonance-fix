//! Suppresses selected effects after the engine has finalized each view.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::{
    bridge::ffi,
    config::PostProcessing,
    utils::{ModuleInfo, SignatureHook, inject_hook},
};

impl From<PostProcessing> for ffi::PostProcessOptions {
    fn from(value: PostProcessing) -> Self {
        Self {
            motion_blur: value.motion_blur,
            depth_of_field: value.depth_of_field,
            bloom: value.bloom,
            film_grain: value.film_grain,
            chromatic_aberration: value.chromatic_aberration,
            vignette: value.vignette,
        }
    }
}

pub(super) fn install(module: &ModuleInfo, config: PostProcessing) {
    if config.motion_blur
        && config.depth_of_field
        && config.bloom
        && config.film_grain
        && config.chromatic_aberration
        && config.vignette
    {
        log::info!("Post processing: all effects use game settings");
        return;
    }

    // Final post-process routine's common epilogue, before restoring RBX.
    // RBX is the scene view; its FPostProcessSettings begins at +0x1320.
    // Native evidence: LEA RDX,[RBX+0x1320] passes the settings to the view
    // extension, and the motion-blur override writes [RBX+0x19F0], matching
    // the SDK's MotionBlurAmount at settings+0x6D0. The hook runs after that
    // override and the optional final adjustment call. See DEVELOPMENT_LOG.
    let hook = SignatureHook {
        tag: "Final post processing",
        signature: "E8 ?? ?? ?? ?? 84 C0 74 08 48 8B CB E8 ?? ?? ?? ?? 48 8B 5C 24 60 48 8B 6C 24 68 48 8B 74 24 70 48 8B 7C 24 78 0F 28 74 24 30 48 83 C4 40 41 5F 41 5E 41 5C C3",
        offset: 17,
    };
    let options = ffi::PostProcessOptions::from(config);
    let reported = AtomicBool::new(false);
    inject_hook(module, &hook, move |ctx| {
        unsafe { ffi::apply_post_processing(ctx.rbx as usize + 0x1320, &options) };
        if !reported.swap(true, Ordering::Relaxed) {
            log::info!("Post processing: first final view processed ({config:?})");
        }
    });
}
