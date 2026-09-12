//! One module per fix: [`aspect_ratio`] patches bytes, [`hud`] owns everything
//! that follows the centered UI root, [`movies`] everything that follows a
//! playing movie.
//!
//! A mid-function hook owns the address it is installed at, so where both fixes
//! correct through the same native callback there can only be one hook. Those
//! three sites are installed here and hand the widget to each fix in turn; the
//! corrections themselves stay in the module they belong to.

mod aspect_ratio;
mod hud;
mod movies;

use crate::{
    config::Config,
    utils::{ModuleInfo, SignatureHook, inject_hook},
};

/// Applies every fix the config enables.
pub fn install(module: &ModuleInfo, config: &Config) {
    aspect_ratio::apply(module);
    if config.hud.enable {
        hud::install(module, config.hud.aspect());
    }
    hook_shared_widget_updates(module, config);
    if config.movies.enable {
        movies::install(module);
    }
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
    if movies || hud {
        let transform = SignatureHook {
            tag: "Widget render transform",
            signature: "48 8B C4 48 89 58 18 48 89 78 20 55 48 8D 68 A1 48 81 EC F0 00 00 00 48 8D 55 B7 48 89 70 10 48 8B D9 E8 ?? ?? ?? ??",
            offset: 0,
        };
        inject_hook(module, &transform, move |ctx| {
            let widget = ctx.rcx as usize;
            if movies {
                movies::on_transform(widget);
            }
            if hud {
                hud::on_transform(widget, aspect);
            }
        });
    }
    // Native UWidget::SetVisibility: RCX is the widget, DL the new visibility.
    let visibility = SignatureHook {
        tag: "Widget visibility change",
        signature: "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 70 0F B6 B1 DC 00 00 00 48 8B D9 0F B6 FA 40 3A F7 74 07 40 88 B9 DC 00 00 00",
        offset: 0,
    };
    let visibility_ready = (movies || hud)
        && inject_hook(module, &visibility, move |ctx| {
            let widget = ctx.rcx as usize;
            if movies {
                movies::on_visibility(widget, ctx.rdx as u8);
            }
            if hud {
                hud::expand_menu_backdrop(widget);
            }
        });
    // Wrapping a movie root is only safe once the wrapper's dismissal is
    // synchronized, so that branch waits on the visibility hook; the dimming
    // scale carries no such dependency.
    let wrap_movies = movies && visibility_ready;
    if wrap_movies || hud {
        // Native UUserWidget rebuild, at the point RBX holds the widget and
        // before its tree root is consumed for the Slate rebuild.
        let rebuild = SignatureHook {
            tag: "User widget rebuild",
            signature: "40 55 53 56 41 57 48 8D AC 24 98 FE FF FF 48 81 EC 68 02 00 00 48 8B 05 ?? ?? ?? ?? 48 33 C4 48 89 85 58 01 00 00",
            offset: 0x4E,
        };
        inject_hook(module, &rebuild, move |ctx| {
            let widget = ctx.rbx as usize;
            if wrap_movies {
                movies::on_rebuild(widget);
            }
            if hud {
                hud::expand_menu_backdrop(widget);
            }
        });
    }
}
