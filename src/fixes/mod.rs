//! One module per fix: [`aspect_ratio`] and [`background`] patch bytes, [`hud`]
//! owns everything that follows the centered UI root, [`map`] the world map's
//! backdrop, [`movies`] everything that follows a playing movie, [`encounter`]
//! the screen capture an encounter transition draws. [`layout`] and [`widgets`]
//! hold what they share.
//!
//! A mid-function hook owns the address it is installed at, so where both fixes
//! correct through the same native callback there can only be one hook. Those
//! three sites are installed here and hand the widget to each fix in turn; the
//! corrections themselves stay in the module they belong to.

mod aspect_ratio;
mod background;
mod encounter;
mod fonts;
mod hud;
mod layout;
mod map;
mod movies;
mod post_processing;
mod widgets;

use std::ffi::c_void;

use sdk::{
    CoreUObject::UObject,
    UMG::{ESlateVisibility, UWidget},
    rt::FName,
};

use crate::{
    config::Config,
    utils::{ModuleInfo, SignatureHook, inject_hook, pattern_scan},
};

/// Applies every fix the config enables.
pub fn install(module: &ModuleInfo, config: &Config) {
    fonts::install(module, &config.fonts);
    aspect_ratio::apply(module);
    if config.background.enable {
        background::apply(module);
    }
    post_processing::install(module, config.post_processing);
    // Everything below reads game objects or names through the SDK.
    if !resolve_sdk(module) {
        log::error!("SDK: lookups not found, HUD, movie and encounter fixes stay off");
        return;
    }
    if config.hud.enable {
        // Re-framing the capture and holding the transition's layout to its
        // shape are halves of one correction, so both follow the HUD flag: a
        // re-framed capture drawn across an unconstrained layout would be
        // stretched by exactly what the re-frame trimmed.
        encounter::install(module);
        hud::install(module, config.hud.aspect());
    }
    hook_shared_widget_updates(module, config);
    if config.movies.enable {
        movies::install(module);
    }
}

/// Points the SDK at the object array and `FName::AppendString` found by
/// signature, so object lookups, class checks and names work wherever the
/// linker places them. Left alone, the SDK reads both at the dump's fixed RVAs.
/// Returns false when either signature is missing.
fn resolve_sdk(module: &ModuleInfo) -> bool {
    // FName::AppendString(FString&): its prologue, then CMP byte ptr [rip+X],0;
    // MOV RSI,RDX (the output string); MOV EBX,[RCX] (the name's comparison
    // index); MOV RDI,RCX. One match image-wide.
    const APPEND_STRING: &str = "48 89 5C 24 ?? 48 89 74 24 ?? 57 48 83 EC ?? 80 3D ?? ?? ?? ?? 00 48 8B F2 8B 19 48 8B F9 74 ??";
    // A chunk lookup into GUObjectArray: MOV RAX,[rip+Objects]; MOV
    // RCX,[RAX+RCX*8]; LEA RAX,[RCX+RDX*8]. The idiom recurs about 300 times
    // and every copy loads the same chunk table, so the first match decodes it.
    const OBJECT_CHUNKS: &str = "48 8B 05 ?? ?? ?? ?? 48 8B 0C C8 48 8D 04 D1";
    let Some(append_string) = pattern_scan(module.address, APPEND_STRING) else {
        log::error!("SDK: FName::AppendString signature not found");
        return false;
    };
    let Some(load) = pattern_scan(module.address, OBJECT_CHUNKS) else {
        log::error!("SDK: GObjects signature not found");
        return false;
    };
    let displacement = unsafe { ((load + 3) as *const i32).read_unaligned() };
    // The SDK addresses the array by its start; the chunk table sits at
    // GOBJECTS_OBJECTS within it.
    let objects =
        (load + 7).wrapping_add_signed(displacement as isize) - sdk::config::GOBJECTS_OBJECTS;
    FName::InitManually(append_string as *mut c_void);
    UObject::GObjects.InitManually(objects as *mut c_void);
    let base = module.address.0 as usize;
    log::info!(
        "SDK: AppendString +{:X} (dump +{:X}), GObjects +{:X} (dump +{:X})",
        append_string - base,
        sdk::config::APPEND_STRING,
        objects - base,
        sdk::config::GOBJECTS
    );
    true
}

/// Installs the native widget callbacks both fixes correct through, each
/// dispatching under the flag its own correction belongs to.
fn hook_shared_widget_updates(module: &ModuleInfo, config: &Config) {
    let (movies, hud) = (config.movies.enable, config.hud.enable);
    let aspect = config.hud.aspect();
    // Native UpdateRenderTransform. RCX is the widget, and the fields it reads
    // -- Translation at +0x90, Scale at +0xA0 -- are both consumed immediately
    // after entry, so a movie's scale and a tooltip's translation are written
    // here in place rather than through a setter that would recurse into it.
    // The frame size and stack offsets are wildcarded.
    if movies || hud {
        let transform = SignatureHook {
            tag: "Widget render transform",
            signature: "48 8B C4 48 89 58 18 48 89 78 20 55 48 8D 68 ?? 48 81 EC ?? ?? ?? ?? 48 8D 55 ?? 48 89 70 10 48 8B D9 E8 ?? ?? ?? ??",
            offset: 0,
        };
        inject_hook(module, &transform, move |ctx| unsafe {
            let widget = ctx.rcx as *mut UWidget;
            if movies {
                movies::on_transform(widget);
            }
            if hud {
                hud::on_transform(widget, aspect);
                // The transition's capture image is the first widget to touch
                // the captured frame after the copy, which is where that frame
                // is re-framed to the shape the transition draws it at.
                encounter::reframe_capture(widget);
            }
        });
    }
    // Native UWidget::SetVisibility: RCX is the widget, DL the new visibility.
    // The read and write of Visibility at +0xDC stay literal; the frame size,
    // the registers they go through and the branch are wildcarded.
    let visibility = SignatureHook {
        tag: "Widget visibility change",
        signature: "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC ?? 0F B6 ?? DC 00 00 00 48 8B ?? 0F B6 ?? ?? 3A ?? 74 ?? ?? 88 ?? DC 00 00 00",
        offset: 0,
    };
    let visibility_ready = (movies || hud)
        && inject_hook(module, &visibility, move |ctx| unsafe {
            let widget = ctx.rcx as *mut UWidget;
            let value = ESlateVisibility(ctx.rdx as u8);
            if movies {
                movies::on_visibility(widget, value);
            }
            if hud {
                hud::expand_menu_backdrop(widget);
                map::update_backdrop(widget, Some(value));
            }
        });
    // Wrapping a movie root is only safe once the wrapper's dismissal is
    // synchronized, so that branch waits on the visibility hook; the dimming
    // scale carries no such dependency.
    let wrap_movies = movies && visibility_ready;
    if wrap_movies || hud {
        // Native UUserWidget rebuild, at the point RBX holds the widget and
        // before its tree root is consumed for the Slate rebuild. The
        // signature runs from the entry to the hook, so every byte before the
        // hook point is checked: MOV RBX,RCX, which puts the widget in RBX,
        // and the LEA RCX,[RBX+...] the hook lands on stay literal, while
        // frame sizes, stack slots, field offsets and the vtable slot are
        // wildcarded.
        let rebuild = SignatureHook {
            tag: "User widget rebuild",
            signature: "40 55 53 56 41 57 48 8D AC 24 ?? ?? ?? ?? 48 81 EC ?? ?? ?? ?? 48 8B 05 ?? ?? ?? ?? 48 33 C4 48 89 85 ?? ?? ?? ?? C7 44 24 ?? 00 00 00 00 4C 8B FA F6 81 ?? ?? ?? ?? ?? 48 8B D9 48 89 BC 24 ?? ?? ?? ?? 75 ?? 48 8B 01 FF 90 ?? ?? ?? ?? 48 8D 8B",
            offset: 0x4E,
        };
        inject_hook(module, &rebuild, move |ctx| unsafe {
            let widget = ctx.rbx as *mut UWidget;
            if wrap_movies {
                movies::on_rebuild(widget.cast());
            }
            if hud {
                hud::expand_menu_backdrop(widget);
                hud::hide_battle_shadow(widget.cast());
                if visibility_ready {
                    map::update_backdrop(widget, None);
                }
            }
        });
    }
}
