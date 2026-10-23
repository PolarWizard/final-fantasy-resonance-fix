//! Suppresses selected effects after the engine has finalized each view.
//!
//! Writes affect the current view; later views receive the same overrides when
//! they reach the hook. Custom materials, HUD artwork, and effects baked into
//! movie frames are outside these controls.

use std::sync::atomic::{AtomicBool, Ordering};

use sdk::Engine::FPostProcessSettings;

use crate::{
    config::PostProcessing,
    utils::{ModuleInfo, SignatureHook, inject_hook},
};

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
    // override and the optional final adjustment call. MOV RCX,RBX and the
    // restore of RBX stay literal; the epilogue's stack offsets and the branch
    // are wildcarded.
    let hook = SignatureHook {
        tag: "Final post processing",
        signature: "E8 ?? ?? ?? ?? 84 C0 74 ?? 48 8B CB E8 ?? ?? ?? ?? 48 8B 5C 24 ?? 48 8B 6C 24 ?? 48 8B 74 24 ?? 48 8B 7C 24 ?? 0F 28 74 24 ?? 48 83 C4 ?? 41 5F 41 5E 41 5C C3",
        offset: 17,
    };
    let reported = AtomicBool::new(false);
    inject_hook(module, &hook, move |ctx| {
        let settings = (ctx.rbx as usize + 0x1320) as *mut FPostProcessSettings;
        apply(unsafe { &mut *settings }, &config);
        if !reported.swap(true, Ordering::Relaxed) {
            log::info!("Post processing: first final view processed ({config:?})");
        }
    });
}

/// Suppresses selected effects in a view's final post-process settings, after
/// the engine has finished blending them.
///
/// Disabled effects have their amount or intensity cleared. Depth of field
/// also clears focal distance, scale, and depth-blur radius: nonpositive focal
/// distance selects the native diaphragm calculation's zero-blur branch, while
/// scale and radius cover the other depth-of-field contributions. Aperture and
/// depth-blur amount remain unchanged, including the amount used as a divisor
/// by the native depth-blur calculation.
///
/// Applying the same options repeatedly is idempotent. Setting an option back
/// to true leaves the supplied value alone; it does not restore a value cleared
/// by an earlier call. Normal values come from the engine's next blended view.
/// These are already-blended view settings, so the override bits, which belong
/// to the earlier blending stage, are left untouched.
fn apply(post: &mut FPostProcessSettings, options: &PostProcessing) {
    if !options.motion_blur {
        post.MotionBlurAmount = 0.0;
    }
    if !options.depth_of_field {
        // The native diaphragm CoC calculation takes its zero-blur branch
        // when focal distance <= 0. Scale also covers the other DOF path;
        // radius suppresses the separate distance-based depth blur.
        post.DepthOfFieldFocalDistance = 0.0;
        post.DepthOfFieldScale = 0.0;
        post.DepthOfFieldDepthBlurRadius = 0.0;
    }
    if !options.bloom {
        post.BloomIntensity = 0.0;
    }
    if !options.film_grain {
        post.FilmGrainIntensity = 0.0;
    }
    if !options.chromatic_aberration {
        post.SceneFringeIntensity = 0.0;
    }
    if !options.vignette {
        post.VignetteIntensity = 0.0;
    }
}
