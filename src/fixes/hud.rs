//! Everything that follows the centered UI root: the root's own viewport slot,
//! the tooltip positions measured against it, and the menu dimming behind it.

use crate::{
    bridge::ffi,
    utils::{ModuleInfo, SignatureHook, inject_hook, log_report},
};

/// Holds the shared UI root inside a centered rectangle of `aspect` by writing
/// its viewport slot's anchors before the game consumes them.
///
/// Three native paths reach that slot and all three are taken: the widget's
/// attachment to the viewport, the native slot update, and the inlined commit
/// inside the reflected setter, which loads the local slot's anchors just after
/// the hook point and so cannot be corrected at the setter's entry.
pub(super) fn install(module: &ModuleInfo, aspect: f64) {
    let attach = SignatureHook {
        tag: "HUD viewport attachment",
        signature: "40 55 56 57 41 54 41 56 48 8D AC 24 60 FC FF FF 48 81 EC A0 04 00 00 48 8B 05 ?? ?? ?? ?? 48 33 C4 48 89 85 60 03 00 00",
        offset: 0,
    };
    inject_hook(module, &attach, move |ctx| {
        log_report(unsafe {
            ffi::constrain_hud_slot(ctx.rdx as usize, ctx.r9 as usize, 0, aspect)
        });
    });
    let native = SignatureHook {
        tag: "HUD native slot update",
        signature: "48 85 D2 0F 84 ?? ?? ?? ?? 55 53 56 57 48 8D 6C 24 C1 48 81 EC D8 00 00 00 8B 42 08 49 8B F8 C1 E8 0F 48 8B DA 48 8B F1",
        offset: 0,
    };
    inject_hook(module, &native, move |ctx| {
        log_report(unsafe {
            ffi::constrain_hud_slot(ctx.rdx as usize, ctx.r8 as usize, 1, aspect)
        });
    });
    let reflected = SignatureHook {
        tag: "HUD reflected slot commit",
        signature: "80 8E D9 00 00 00 20 48 8B F8 0F 28 45 90 0F 28 4D A0 0F 11 00 0F 28 45 B0 0F 11 48 10",
        offset: 10,
    };
    inject_hook(module, &reflected, move |ctx| {
        // RSI is the widget, RBP-0x70 the local slot the commit copies.
        let slot = (ctx.rbp - 0x70) as usize;
        log_report(unsafe { ffi::constrain_hud_slot(ctx.rsi as usize, slot, 2, aspect) });
    });
}

/// Takes the root's inset back out of a tooltip's render translation, from the
/// shared render-transform hook that is about to consume the field.
pub(super) fn on_transform(widget: usize, aspect: f64) {
    log_report(unsafe { ffi::correct_tooltip_position(widget, aspect) });
}

/// Scales a menu's dimming image across the whole viewport, from both shared
/// hooks that reach one: a menu shown again reuses its dimming widget, by then
/// arranged, while a rebuilt one arrives before Slate has arranged anything.
pub(super) fn expand_menu_backdrop(widget: usize) {
    log_report(unsafe { ffi::expand_menu_backdrop(widget) });
}
