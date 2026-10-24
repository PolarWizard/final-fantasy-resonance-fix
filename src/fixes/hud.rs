//! Everything that follows the centered UI root: the root's own viewport slot,
//! the tooltip positions measured against it, the menu dimming behind it, and
//! the battle layout's corner shadow.

use std::{
    cell::RefCell,
    collections::HashMap,
    ptr::{self, null_mut},
};

use sdk::{
    CoreUObject::{FVector2D, UClass, UObject},
    SlateCore::{EHorizontalAlignment, ESlateColorStylingMode, EVerticalAlignment, FMargin},
    UMG::{
        ESlateVisibility, FGameViewportWidgetSlot, UImage, UOverlay, UOverlaySlot, UPanelSlot,
        UPanelWidget, USlateBlueprintLibrary, UUserWidget, UWidget, UWidgetSwitcher,
        UWidgetSwitcherSlot, UWidgetTree,
    },
    WBP_OverAllLayout::UWBP_OverAllLayout_C,
    mucha::{UCPP_WBP_TalkEvent, UCPP_WBP_Tooltip},
    rt::NameCache,
};

use super::{
    encounter,
    layout::{OffsetState, Scale, apply_offset, letterbox_inset, side_bleed_scale},
    widgets::{
        apply_scale, approximately, arranged, describe, expand_backdrop, is_mod_backdrop,
        layout_box, live, outward, viewport_local_size,
    },
};
use crate::utils::{ModuleInfo, SignatureHook, inject_hook, log_report};

/// The authored render translation of one tooltip, one axis each.
///
/// [`apply_offset`] re-derives the baseline from any value it did not write
/// itself, so an entry must outlive the widget's correction: dropping it while
/// the tooltip still carries the inset would treat the corrected translation as
/// authored and subtract the inset a second time.
#[derive(Default)]
struct Baseline {
    x: OffsetState,
    y: OffsetState,
}

thread_local! {
    static TOOLTIP_BASELINES: RefCell<HashMap<String, Baseline>> = RefCell::new(HashMap::new());
}

/// Holds the shared UI root inside a centered rectangle of `aspect` by writing
/// its viewport slot's anchors before the game consumes them.
///
/// Three native paths reach that slot and all three are taken: the widget's
/// attachment to the viewport, the native slot update, and the inlined commit
/// inside the reflected setter, which loads the local slot's anchors just after
/// the hook point and so cannot be corrected at the setter's entry.
///
/// The two entry hooks wildcard frame sizes, stack offsets and registers the
/// body only moves values through; the arguments they read arrive in the
/// calling convention's registers either way. The attachment's prologue alone
/// is common, so its signature continues through the moves that park its
/// arguments.
pub(super) fn install(module: &ModuleInfo, aspect: f64) {
    let attach = SignatureHook {
        tag: "HUD viewport attachment",
        signature: "40 55 56 57 41 54 41 56 48 8D AC 24 ?? ?? ?? ?? 48 81 EC ?? ?? ?? ?? 48 8B 05 ?? ?? ?? ?? 48 33 C4 48 89 85 ?? ?? ?? ?? 4C 89 44 24 ?? 4D 8B F1 48 89 4D ?? 4C 8B E2",
        offset: 0,
    };
    inject_hook(module, &attach, move |ctx| unsafe {
        constrain_hud_slot(ctx.rdx as *mut UObject, ctx.r9 as *mut _, 0, aspect);
    });
    let native = SignatureHook {
        tag: "HUD native slot update",
        signature: "48 85 D2 0F 84 ?? ?? ?? ?? 55 53 56 57 48 8D 6C 24 ?? 48 81 EC ?? ?? ?? ?? 8B 42 08 49 8B ?? C1 E8 0F 48 8B ?? 48 8B ??",
        offset: 0,
    };
    inject_hook(module, &native, move |ctx| unsafe {
        constrain_hud_slot(ctx.rdx as *mut UObject, ctx.r8 as *mut _, 1, aspect);
    });
    // RSI and the local slot at RBP-0x70 are what the hook reads, so the
    // instructions naming them stay literal.
    let reflected = SignatureHook {
        tag: "HUD reflected slot commit",
        signature: "80 8E D9 00 00 00 20 48 8B ?? 0F 28 45 90 0F 28 4D A0 0F 11 00 0F 28 45 B0 0F 11 48 10",
        offset: 10,
    };
    inject_hook(module, &reflected, move |ctx| unsafe {
        // RSI is the widget, RBP-0x70 the local slot the commit copies.
        let slot = (ctx.rbp - 0x70) as *mut FGameViewportWidgetSlot;
        constrain_hud_slot(ctx.rsi as *mut UObject, slot, 2, aspect);
    });
}

/// Holds a viewport root inside a centered rectangle of `aspect`.
///
/// Anchors are ratios of the viewport, so measuring it in local units gives the
/// identical rectangle at any resolution or DPI, and records the earliest
/// viewport every later correction can fall back on. `path` names which of the
/// three hooked paths called, for the log.
///
/// # Safety
///
/// `widget` is null or a UObject, and `slot` the `FGameViewportWidgetSlot` the
/// game reads after the hook, both taken from the hook's registers on the game
/// thread.
unsafe fn constrain_hud_slot(
    widget: *mut UObject,
    slot: *mut FGameViewportWidgetSlot,
    path: u32,
    aspect: f64,
) {
    // Path 1 hooks the slot update at its entry, and that function opens with
    // TEST RDX,RDX / JZ -- the first bytes of its own signature. The engine
    // calls it with a null widget, so the hook sees one before the engine's
    // test does.
    if !live(widget) {
        return;
    }
    // The encounter transition draws the captured frame as a picture of the
    // capture's own shape, and encounter.rs re-frames that frame to be one, so
    // this layout is held to the capture's shape rather than the HUD's. Its
    // Blueprint class is loaded on demand, so it is recognised by the name the
    // live class carries.
    static ENCOUNTER: NameCache = NameCache::new("WBP_EncountLayout_C");
    // The chapter title an in-engine cutscene opens with arrives here as a
    // viewport root of its own, WBP_ChapterBegin_C, rather than as a screen
    // inside the overall layout, so it is held to the HUD's rectangle
    // separately. It is loaded on demand like the encounter layout.
    static CHAPTER_TITLE: NameCache = NameCache::new("WBP_ChapterBegin_C");
    let class = unsafe { &*(*widget).Class };
    let aspect = if class.Name == ENCOUNTER.get() {
        encounter::captured_aspect()
    } else if ptr::eq(class, UWBP_OverAllLayout_C::StaticClass())
        || class.Name == CHAPTER_TITLE.get()
    {
        aspect
    } else {
        let name = class.GetName();
        log_report(
            &format!("viewport_root:{name}"),
            format!("Viewport root left unconstrained: {name}"),
        );
        return;
    };
    let viewport = viewport_local_size(widget);
    if !arranged(&viewport) {
        return;
    }

    let inset = letterbox_inset(viewport.X, viewport.Y, aspect);
    let slot = unsafe { &mut *slot };
    slot.Anchors.Minimum = FVector2D::new(inset.x / viewport.X, inset.y / viewport.Y);
    slot.Anchors.Maximum =
        FVector2D::new(1.0 - slot.Anchors.Minimum.X, 1.0 - slot.Anchors.Minimum.Y);
    slot.Offsets = FMargin {
        Left: 0.0,
        Top: 0.0,
        Right: 0.0,
        Bottom: 0.0,
    };
    slot.Alignment = FVector2D::new(0.0, 0.0);

    let class_name = class.GetName();
    log_report(
        &format!("native_hud:{class_name}:{path}"),
        format!(
            "Native HUD slot: path={path}, root={}, aspect={aspect}, screen={}x{}, anchors={},{}/{},{}",
            unsafe { (*widget).GetName() },
            viewport.X,
            viewport.Y,
            slot.Anchors.Minimum.X,
            slot.Anchors.Minimum.Y,
            slot.Anchors.Maximum.X,
            slot.Anchors.Maximum.Y
        ),
    );
}

/// Walks outward from a tooltip to the constrained UI root.
///
/// Returns `None` when a tooltip ancestor comes first, since this tooltip then
/// already moves with it, and otherwise the root, or null when it is not in the
/// chain yet. `tooltip_class` is compared against each step.
fn find_constrained_root(widget: *mut UWidget, tooltip_class: *mut UClass) -> Option<*mut UWidget> {
    let overall_class = UWBP_OverAllLayout_C::StaticClass();
    let mut node = outward(widget);
    while !node.is_null() {
        let current = unsafe { &*node };
        if current.IsA(tooltip_class) {
            return None;
        }
        if current.Class == overall_class {
            return Some(node);
        }
        node = outward(node);
    }
    Some(null_mut())
}

/// How far the constrained root sits in from each viewport edge, in the same
/// units as `screen`.
///
/// The root's own arranged box is the measurement once Slate has produced one.
/// A tooltip the game positions before attaching it has no root in its chain
/// yet, and there the rectangle the slot hook anchors the root to is the same
/// inset, so the letterbox geometry stands in for the measurement.
fn tooltip_inset(root_box: &FVector2D, screen: &FVector2D, aspect: f64) -> FVector2D {
    if arranged(root_box) {
        return FVector2D::new((screen.X - root_box.X) * 0.5, (screen.Y - root_box.Y) * 0.5);
    }
    let centered = letterbox_inset(screen.X, screen.Y, aspect);
    FVector2D::new(centered.x, centered.y)
}

/// Takes the constrained root's inset back out of a tooltip's position, from
/// the shared render-transform hook that is about to consume the field.
///
/// The game positions a tooltip with a render translation it computes in
/// viewport coordinates, but the widget hangs under a root whose slot anchors
/// hold it inside `aspect`, so the translation lands one root inset too far in.
/// The inset comes back out exactly once, tracked per widget by
/// [`apply_offset`], which keeps the game's authored value as the baseline and
/// rewrites baseline+inset idempotently however often the transform updates.
///
/// Everything here is in local (UMG) units, which is what a render translation
/// is expressed in: at 7680x2160 with viewport scale 2 the viewport is 3840x1080
/// local units and the root arranges to 1920x1080, so the inset is 960,0.
///
/// # Safety
///
/// `widget` is the widget in RCX at native UpdateRenderTransform, on the game
/// thread.
pub(super) unsafe fn on_transform(widget: *mut UWidget, aspect: f64) {
    let tooltip_class = UCPP_WBP_Tooltip::StaticClass();
    // A blueprint's template widget reaches these callbacks alongside its
    // instances, and this one writes to the widget: a translation written onto
    // a template is inherited by every instance spawned from it afterwards.
    // live() rejects defaults and archetypes for that reason, not to doubt the
    // pointer the engine passed.
    if !live(widget) || !unsafe { (*widget).IsA(tooltip_class) } {
        return;
    }
    let Some(root) = find_constrained_root(widget, tooltip_class) else {
        return;
    };

    // The root is attached, so it resolves the viewport even while the tooltip
    // itself is being built and cannot.
    let context = if root.is_null() { widget } else { root };
    let screen = viewport_local_size(context.cast());
    if !arranged(&screen) {
        return;
    }

    let root_box = if root.is_null() {
        FVector2D::new(0.0, 0.0)
    } else {
        unsafe { USlateBlueprintLibrary::GetLocalSize(&(*root).GetCachedGeometry()) }
    };
    let inset = tooltip_inset(&root_box, &screen, aspect);

    let key = {
        let tooltip = unsafe { &*widget };
        format!("{}:{}", tooltip.GetFullName(), tooltip.Index)
    };
    let before = unsafe { (*widget).RenderTransform.Translation };
    let corrected = TOOLTIP_BASELINES.with_borrow_mut(|baselines| {
        // An unconstrained root moves nothing, so no entry is needed until
        // there is an inset to keep consistent.
        if !baselines.contains_key(&key) && inset.X < 0.5 && inset.Y < 0.5 {
            return None;
        }
        let baseline = baselines.entry(key.clone()).or_default();
        let applied = FVector2D::new(
            f64::from(apply_offset(
                &mut baseline.x,
                before.X as f32,
                -inset.X as f32,
            )),
            f64::from(apply_offset(
                &mut baseline.y,
                before.Y as f32,
                -inset.Y as f32,
            )),
        );
        Some((applied, baseline.x.baseline, baseline.y.baseline))
    });
    let Some((applied, authored_x, authored_y)) = corrected else {
        return;
    };
    // Native UpdateRenderTransform consumes this field immediately after the hook.
    unsafe { (*widget).RenderTransform.Translation = applied };

    log_report(
        &format!("tooltip_position:{key}"),
        format!(
            "Tooltip position: {}, root_box={}x{}{}, screen={}x{}, inset={},{}, authored={},{}, applied={},{}",
            unsafe { &*widget }.GetName(),
            root_box.X,
            root_box.Y,
            if root.is_null() {
                " (root not in chain)"
            } else {
                ""
            },
            screen.X,
            screen.Y,
            inset.X,
            inset.Y,
            authored_x,
            authored_y,
            applied.X,
            applied.Y
        ),
    );
}

// A top-level screen is owned by the overall layout's widget tree, or by a
// talk event's: UCPP_WBP_TalkEvent carries its own WidgetStack and dialog
// classes, so a dialog an in-engine cutscene opens is created in the talk
// event's tree and laid out across it. Nested controls have another
// UserWidget owner and must keep their authored bounds.
fn top_level_screen(screen: *mut UUserWidget) -> bool {
    let tree = unsafe { &*screen }.Outer;
    unsafe {
        let owner = if live(tree) && (*tree).IsA(UWidgetTree::StaticClass()) {
            (*tree).Outer
        } else {
            null_mut()
        };
        live(owner)
            && ((*owner).Class == UWBP_OverAllLayout_C::StaticClass()
                || (*owner).IsA(UCPP_WBP_TalkEvent::StaticClass()))
    }
}

fn unpositioned(widget: *mut UWidget) -> bool {
    let transform = unsafe { &(*widget).RenderTransform };
    approximately(transform.Translation.X, 0.0)
        && approximately(transform.Translation.Y, 0.0)
        && approximately(f64::from(transform.Angle), 0.0)
        && approximately(transform.Shear.X, 0.0)
        && approximately(transform.Shear.Y, 0.0)
}

// The horizontal alignment a slot gives its content when that content fills
// the parent's height with no padding, for the two slot kinds a screen's
// background passes through: an overlay layer and a switcher page.
fn full_height_alignment(slot: *mut UPanelSlot) -> Option<EHorizontalAlignment> {
    if !live(slot) {
        return None;
    }
    let (padding, horizontal, vertical) = unsafe {
        if (*slot).IsA(UOverlaySlot::StaticClass()) {
            let slot = &*slot.cast::<UOverlaySlot>();
            (
                slot.Padding,
                slot.HorizontalAlignment,
                slot.VerticalAlignment,
            )
        } else if (*slot).IsA(UWidgetSwitcherSlot::StaticClass()) {
            let slot = &*slot.cast::<UWidgetSwitcherSlot>();
            (
                slot.Padding,
                slot.HorizontalAlignment,
                slot.VerticalAlignment,
            )
        } else {
            return None;
        }
    };
    let unpadded = [padding.Left, padding.Right, padding.Top, padding.Bottom]
        .iter()
        .all(|&side| approximately(f64::from(side), 0.0));
    (unpadded && vertical == EVerticalAlignment::VAlign_Fill).then_some(horizontal)
}

// UImage draws both shades and artwork. A resource-free brush is a flat fill,
// and one drawn near black is a shade whatever texture shapes it -- artwork
// drawn that way would show nothing of itself. Textures, materials (including
// dynamic instances) and named or dynamically loaded images drawn in colour
// stay authored.
fn shade(image: &UImage) -> bool {
    let brush = &image.Brush;
    let solid = brush.ResourceObject.is_null()
        && brush.ResourceName.IsNone()
        && !brush.bIsDynamicallyLoaded();
    let (tint, color) = (&brush.TintColor.SpecifiedColor, &image.ColorAndOpacity);
    let dark = brush.TintColor.ColorUseRule == ESlateColorStylingMode::UseColor_Specified
        && [tint.R * color.R, tint.G * color.G, tint.B * color.B]
            .iter()
            .all(|&channel| channel <= 0.05);
    solid || dark
}

/// A shade behind a screen's controls, and the alignment its slot gives it:
/// fill for one across the whole box, left or right for one pinned to a side.
type Shade = (*mut UImage, EHorizontalAlignment);

// Follows the backmost layer of each overlay and every page of a switcher,
// crossing reusable widget trees, to the shades painted behind the screen's
// controls. Every page fills its switcher, so the hidden ones are corrected
// before they are shown. Each container on the way fills its parent; only the
// leaf image may be pinned to a side, and only the image is scaled, never a
// container that could also carry controls. `covered` records that an overlay
// on the way paints something in front of the branch, which a screen that is
// one picture -- a movie -- never does.
fn collect_shades(
    node: *mut UWidget,
    align: EHorizontalAlignment,
    covered: bool,
    depth: u32,
    found: &mut Vec<Shade>,
) {
    if depth > 32 || !live(node) || !unpositioned(node) {
        return;
    }
    unsafe {
        if (*node).IsA(UImage::StaticClass()) {
            let image = node.cast::<UImage>();
            if covered
                && align != EHorizontalAlignment::HAlign_Center
                && shade(&*image)
                && !is_mod_backdrop(node)
            {
                found.push((image, align));
            }
            return;
        }
        if align != EHorizontalAlignment::HAlign_Fill {
            return;
        }
        if (*node).IsA(UUserWidget::StaticClass()) {
            let tree = (*node.cast::<UUserWidget>()).WidgetTree;
            if live(tree) {
                collect_shades((*tree).RootWidget, align, covered, depth + 1, found);
            }
            return;
        }
        let switcher = (*node).IsA(UWidgetSwitcher::StaticClass());
        if !switcher && !(*node).IsA(UOverlay::StaticClass()) {
            return;
        }
        let slots = (*node.cast::<UPanelWidget>()).Slots.as_slice();
        let branches = if switcher {
            slots
        } else {
            &slots[..slots.len().min(1)]
        };
        let covered = covered || (!switcher && slots.len() >= 2);
        for &slot in branches {
            if let Some(align) = full_height_alignment(slot) {
                collect_shades((*slot).Content, align, covered, depth + 1, found);
            }
        }
    }
}

// The tree root of the top-level screen `widget` belongs to, or null. A
// reusable component's rebuild or show also reaches its owning screen. This is
// event-driven and does not cache widgets across collection.
fn menu_screen_root(widget: *mut UWidget) -> *mut UWidget {
    unsafe {
        if !(*widget).IsA(UUserWidget::StaticClass()) && !(*widget).IsA(UImage::StaticClass()) {
            return null_mut();
        }
    }
    let mut node = widget;
    for _ in 0..32 {
        if !live(node) {
            break;
        }
        unsafe {
            if (*node).IsA(UUserWidget::StaticClass()) && top_level_screen(node.cast()) {
                let tree = (*node.cast::<UUserWidget>()).WidgetTree;
                return if live(tree) {
                    (*tree).RootWidget
                } else {
                    null_mut()
                };
            }
        }
        node = outward(node);
    }
    null_mut()
}

/// Carries a full-height shade pinned to one side of the screen's box out to
/// that side of the viewport, its inner edge held where it is: the battle
/// result's reward page darkens its left with a 750-wide gradient, which would
/// otherwise end in a hard edge where the box begins.
fn extend_to_side(image: *mut UImage, align: EHorizontalAlignment, key: &str, label: &str) {
    // The shade is pinned within its parent, which has the screen's box since
    // every container above it fills its own.
    let parent = outward(image.cast());
    if parent.is_null() {
        return;
    }
    let layout = layout_box(parent);
    if !arranged(&layout.size) || !arranged(&layout.screen) {
        return;
    }
    // A pinned image is as wide as its brush.
    let width = f64::from(unsafe { &*image }.Brush.ImageSize.X);
    let scale = Scale {
        x: side_bleed_scale(
            width,
            layout.size.X,
            layout.size.Y,
            layout.screen.X,
            layout.screen.Y,
        ),
        y: 1.0,
    };
    let inner = if align == EHorizontalAlignment::HAlign_Left {
        1.0
    } else {
        0.0
    };
    if apply_scale(image.cast(), FVector2D::new(inner, 0.5), scale) {
        log_report(
            key,
            format!("{label}: width={width}, {}", describe(&layout, scale)),
        );
    }
}

/// Paints a menu's shades out to the viewport's edges, from both shared hooks
/// that reach one: a menu shown again reuses its shade, by then arranged,
/// while a rebuilt one arrives before Slate has arranged anything.
///
/// A menu shades the scene behind it with an image under its controls, most
/// often a full-bleed WBP_BGDimming: logs record it inside
/// WBP_SystemMenuLayout_C, WBP_OptionLayout_C, WBP_DialogLayout_C,
/// WBP_LoadLayout_C and WBP_TutorialLayout_C, and in the battle result's growth
/// pages behind a page switcher. The root's inset leaves the letterbox columns
/// unshaded, so a full-bleed shade is render-scaled about its center -- a 16:9
/// root on a 32:9 screen by 2,1 -- and one pinned to a side is stretched out to
/// that side. Layout, hit testing and every sibling stay exactly as authored.
///
/// Shades are recognized by their position behind the screen's controls,
/// including across reusable widget trees and switcher pages, without
/// widget-name rules.
///
/// # Safety
///
/// `widget` is the widget in RCX at SetVisibility or RBX at the widget rebuild,
/// on the game thread.
pub(super) unsafe fn expand_menu_backdrop(widget: *mut UWidget) {
    // The blueprint's own WBP_BGDimming template arrives here as well as each
    // instance, and a render scale written to the template would be handed to
    // every menu spawned later.
    if !live(widget) {
        return;
    }
    let root = menu_screen_root(widget);
    if root.is_null() {
        return;
    }
    let mut shades = Vec::new();
    collect_shades(
        root,
        EHorizontalAlignment::HAlign_Fill,
        false,
        0,
        &mut shades,
    );
    // On rebuild a shade has no geometry yet, and the measuring walks reach the
    // root: the shade's box is the root's once arranged, and the root's player
    // context resolves the viewport the shade itself cannot yet report.
    for (image, align) in shades {
        let name = unsafe { (*image).GetFullName() };
        let key = format!("menu_backdrop:{name}");
        let label = format!("Menu backdrop: {name}");
        if align == EHorizontalAlignment::HAlign_Fill {
            expand_backdrop(image.cast(), &key, &label);
        } else {
            extend_to_side(image, align, &key, &label);
        }
    }
}

/// The image the battle layout draws its corner shadow with, or null if this
/// branch does not hold it.
///
/// Searched by the name it carries and confirmed by the texture it draws, since
/// a name alone only suggests one. Depth-bounded, and confined to the tree it
/// is given, which is the layout's own: the image belongs to that tree rather
/// than to any component nested in it.
fn battle_shadow(node: *mut UWidget, depth: u32) -> *mut UImage {
    static SHADOW_NAME: NameCache = NameCache::new("RU_BG");
    if !live(node) || depth > 16 {
        return null_mut();
    }
    let current = unsafe { &*node };
    unsafe {
        if current.Name == SHADOW_NAME.get() && current.IsA(UImage::StaticClass()) {
            let texture = (*node.cast::<UImage>()).Brush.ResourceObject;
            return if live(texture) && (*texture).GetName() == "T_Btl_layout_shadow" {
                node.cast()
            } else {
                null_mut()
            };
        }
        if !(*node).IsA(UPanelWidget::StaticClass()) {
            return null_mut();
        }
        let panel = node.cast::<UPanelWidget>();
        for i in 0..(*panel).GetChildrenCount() {
            let found = battle_shadow((*panel).GetChildAt(i), depth + 1);
            if !found.is_null() {
                return found;
            }
        }
    }
    null_mut()
}

/// Stops the battle layout drawing a shadow inside the screen.
///
/// WBP_BattleLayout_C paints an image across itself that draws
/// T_Btl_layout_shadow, a darkening that fades out towards the layout's
/// corners. Those corners are the screen's on a 16:9 display, where the fade
/// reads as vignetting; held inside 16:9 on a wider one they sit in open
/// screen, and the fade becomes a shadow with a visible edge. Hiding the image
/// keeps it and its layout space and stops it drawing.
///
/// Applied when the layout is rebuilt, which is the moment its tree is
/// populated and Slate has not yet consumed it, and once is enough: a battle
/// run with this reported one change and no second, so nothing shows the image
/// again for the rest of the battle.
///
/// # Safety
///
/// `layout` is the widget being rebuilt, from RBX, on the game thread.
pub(super) unsafe fn hide_battle_shadow(layout: *mut UUserWidget) {
    static BATTLE_LAYOUT: NameCache = NameCache::new("WBP_BattleLayout_C");
    if !live(layout) {
        return;
    }
    let layout = unsafe { &*layout };
    if unsafe { &*layout.Class }.Name != BATTLE_LAYOUT.get() || !live(layout.WidgetTree) {
        return;
    }
    let shadow = battle_shadow(unsafe { (*layout.WidgetTree).RootWidget }, 0);
    if shadow.is_null() {
        return;
    }
    let shadow = unsafe { &mut *shadow };
    if shadow.Visibility == ESlateVisibility::Hidden {
        return;
    }
    shadow.SetVisibility(ESlateVisibility::Hidden);
    log_report(
        "battle_shadow",
        format!(
            "Battle shadow: hid {} in {}",
            shadow.GetName(),
            layout.GetName()
        ),
    );
}
