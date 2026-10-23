//! Everything that follows a playing movie: fitting its image to 16:9 and the
//! black backdrop wrapped behind it.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ptr::null_mut,
};

use sdk::{
    CoreUObject::{FVector2D, UObject},
    UMG::{ESlateVisibility, UOverlay, UUserWidget, UWidget},
    mucha::{
        EeBtlSequenceEventType, FBTL_SEQUENCE_PARAMETER, UCPP_WBP_MovieProjector,
        UCPP_WBP_MovieScreen,
    },
    rt::NameCache,
};

use super::{
    layout::fit_16_9,
    widgets::{
        BackdropWrapper, WrapGuard, apply_center_scale, approximately, arranged, describe,
        expand_backdrop, fill_overlay, layout_box, live, live_backdrop, mirrored_visibility, spawn,
        spawn_backdrop_image,
    },
};
use crate::utils::{ModuleInfo, SignatureHook, inject_hook, log_report};

static MOVIE_NAME: NameCache = NameCache::new("Image_Movie");

thread_local! {
    /// Every movie root this mod has wrapped, by the root's full name.
    static WRAPPED_ROOTS: RefCell<HashMap<String, BackdropWrapper>> = RefCell::new(HashMap::new());

    /// Set while [`on_rebuild`] spawns widgets, so the visibility changes that
    /// spawning triggers are not mistaken for the game dismissing a movie.
    static WRAPPING: Cell<bool> = const { Cell::new(false) };

    /// Whether a sequence has put a movie on screen, as the sequence itself says.
    ///
    /// A summon authors UISetVisibility with its flag clear when its cinematic
    /// takes the whole screen, and a sequence that names its movie directly
    /// authors UISetVisibilityMovie. The later event wins, which is the
    /// sequence's latest instruction either way.
    static MOVIE_PHASE: Cell<bool> = const { Cell::new(false) };
}

/// Fits a playing movie to 16:9 from the native callbacks that own its image:
/// the movie screen's frame update and texture assignment, and the point where
/// creating a movie screen completes.
///
/// The Projector field at +0x368, which `on_native_movie` reads through the
/// SDK, stays literal in the first two, as does the RBX the completion hook
/// reads; frame sizes, branches and moved-through registers are wildcarded.
pub(super) fn install(module: &ModuleInfo) {
    let frame = SignatureHook {
        tag: "MovieScreen::OnFrame",
        signature: "40 53 48 83 EC ?? 48 8B 99 68 03 00 00 48 8B 8B 68 03 00 00 E8 ?? ?? ?? ?? 48 85 C0 74 ??",
        offset: 0,
    };
    inject_hook(module, &frame, |ctx| {
        on_native_movie(ctx.rcx as *mut UObject, true);
    });
    let texture = SignatureHook {
        tag: "MovieProjector::SetManaTexture",
        signature: "48 89 5C 24 10 57 48 83 EC ?? 48 8B ?? 48 8B ?? 48 8B 89 68 03 00 00 E8 ?? ?? ?? ??",
        offset: 0,
    };
    inject_hook(module, &texture, |ctx| {
        // RDX is the incoming texture; a cleared one starts no playback.
        if ctx.rdx == 0 {
            return;
        }
        on_native_movie(ctx.rcx as *mut UObject, false);
    });
    let created = SignatureHook {
        tag: "CreateMovieScreen completion",
        signature: "33 D2 48 89 5F ?? 48 8B CB E8 ?? ?? ?? ?? 48 8B 47 ?? 48 83 C4 ?? 5F 5E 5B C3",
        offset: 14,
    };
    inject_hook(module, &created, |ctx| {
        // RBX holds the screen the creation stored, null when it failed.
        if ctx.rbx == 0 {
            return;
        }
        on_native_movie(ctx.rbx as *mut UObject, true);
    });
    // A battle sequence dispatches its authored events through one native
    // function, tagged by the EventType byte at the head of the parameter
    // struct. The signature is that dispatch reading the tag and bounding it
    // against the last event type: MOVZX EAX,byte ptr [R8]; DEC EAX;
    // CMP EAX,0x7A. The bound stays literal: wildcarded, it also matches a
    // nine-case dispatch with the same read. Hooking the read rather than the
    // function entry keeps the anchor adjacent to the field that is read, and
    // R8 still holds the struct there.
    let sequence = SignatureHook {
        tag: "Battle sequence event",
        signature: "41 0F B6 00 FF C8 83 F8 7A 0F 87 ?? ?? ?? ??",
        offset: 0,
    };
    inject_hook(module, &sequence, |ctx| {
        on_battle_sequence_event(ctx.r8 as *const FBTL_SEQUENCE_PARAMETER);
    });
}

/// Whether a wrapped root is a cinematic's movie image.
///
/// A cinematic's movie image shares its layout with an in-engine lead-in, so
/// its backdrop waits for the sequence. A movie screen or movie widget shows
/// nothing but a movie, and keeps following its own visibility.
fn waits_for_sequence(root: *const UWidget) -> bool {
    !root.is_null() && unsafe { &*root }.Name == MOVIE_NAME.get()
}

/// Whether the black image behind a movie should be drawn yet.
///
/// The gate belongs on the backdrop alone. The wrapper it hangs in is the
/// cinematic's widget-tree root and holds the movie image as its other child,
/// so hiding the wrapper takes the movie down with the backdrop and the
/// cinematic shows nothing at all. Hidden rather than Collapsed keeps the
/// image arranged, so the geometry the expansion measures stays available and
/// the bars are already the right size when they are drawn.
fn backdrop_visibility(root: *const UWidget) -> ESlateVisibility {
    if waits_for_sequence(root) && !MOVIE_PHASE.get() {
        ESlateVisibility::Hidden
    } else {
        ESlateVisibility::HitTestInvisible
    }
}

/// Re-gates and re-expands every backdrop this mod has wrapped.
///
/// Two things are decided per wrapper and both are absolute, so running this
/// from any callback converges rather than accumulating. Whether the backdrop
/// is drawn is the gate, since the sequence event that flips it names no widget
/// and each wrapper has to be asked again; how far it is stretched is the
/// expansion, because a wrapper held inside the constrained root is arranged
/// 16:9 and the letterbox columns outside it would otherwise show the world
/// instead of black.
///
/// The movie a wrapper holds is the second child of its overlay and the
/// backdrop the first, so both are derived from the live widget and the
/// registry keeps holding identities only. The registry is copied out first,
/// since the setters below re-enter the hooks that read it.
fn refresh_movie_backdrops() {
    let wrappers: Vec<_> = WRAPPED_ROOTS.with_borrow(|roots| {
        roots
            .iter()
            .map(|(name, wrapper)| (name.clone(), wrapper.clone()))
            .collect()
    });
    for (root_name, wrapper) in wrappers {
        let black = live_backdrop(&wrapper);
        if black.is_null() {
            continue;
        }
        unsafe {
            let image = &mut *black;
            let overlay = if live(image.Slot) {
                (*image.Slot).Parent
            } else {
                null_mut()
            };
            let root = if live(overlay) && (*overlay).GetChildrenCount() > 1 {
                (*overlay).GetChildAt(1)
            } else {
                null_mut()
            };
            if live(root) {
                let value = backdrop_visibility(root);
                if image.Visibility != value {
                    image.SetVisibility(value);
                    log_report(
                        &format!("gate:{root_name}"),
                        format!(
                            "Movie backdrop gate: {root_name}, drawn={}",
                            value == ESlateVisibility::HitTestInvisible
                        ),
                    );
                }
            }
        }
        expand_backdrop(
            black.cast(),
            &format!("backdrop:{root_name}"),
            &format!("Movie backdrop: {root_name}"),
        );
    }
}

/// The widget-tree root to wrap, or null when this is not a movie or cannot be
/// wrapped.
fn wrappable_movie_root(user: *mut UUserWidget) -> *mut UWidget {
    unsafe {
        let tree = (*user).WidgetTree;
        if tree.is_null() || (*tree).RootWidget.is_null() {
            return null_mut();
        }
        let root = (*tree).RootWidget;
        let name = (*root).GetName();
        // Only wrap a widget-tree root, never reparent a content child.
        if (name != "Image_Movie" && name != "Projector") || !(*root).Slot.is_null() {
            return null_mut();
        }
        root
    }
}

/// Carries a battle sequence's own decisions to the movie backdrop.
///
/// A summon builds its cinematic and makes the movie image Visible when the
/// action starts, then plays an in-engine animation over the battle before the
/// movie itself appears, so neither the widget's visibility nor Mana's playback
/// state marks the cut: a capture of a real summon has visibility, both
/// bUseMovie flags and a Mana component all arriving together at the start of
/// the lead-in. The sequence authors its own events instead.
/// FBTL_SEQUENCE_PARAMETER is a tagged union whose EventType selects which of
/// its fields apply.
///
/// A run of this hook shows the summon authoring UISetVisibility with its flag
/// clear 1.07s after the cinematic was wrapped, and no UISetVisibilityMovie at
/// all, so the battle UI being hidden is what marks the movie phase: the
/// lead-in is a camera move over a battle whose HUD is still up, and hiding
/// that HUD is the cinematic taking the whole screen. A sequence that names
/// its movie directly is honoured through the other event.
///
/// `event` is the FBTL_SEQUENCE_PARAMETER, from R8 where the dispatch reads its
/// EventType.
fn on_battle_sequence_event(event: *const FBTL_SEQUENCE_PARAMETER) {
    if event.is_null() {
        return;
    }
    let event = unsafe { &*event };
    let named = match event.EventType {
        EeBtlSequenceEventType::UISetVisibilityMovie => {
            MOVIE_PHASE.set(event.UI_SetVisibillityMovie_IsVisibility);
            "movie visibility"
        }
        EeBtlSequenceEventType::UISetVisibility => {
            MOVIE_PHASE.set(!event.UI_SetVisibillity_IsVisibility);
            "battle UI hidden"
        }
        _ => return,
    };
    let phase = if MOVIE_PHASE.get() { "on" } else { "off" };
    log_report("movie_phase", format!("Battle sequence: {named} {phase}"));
    // The event names no widget, so every wrapper is asked again.
    refresh_movie_backdrops();
}

/// Letterboxes a summon's movie image to 16:9, from the shared render-transform
/// hook that is about to consume the scale field.
///
/// The image is fitted to its own arranged box, not to the viewport: an
/// ancestor held inside 16:9 already hands it a 16:9 box, and scaling that by
/// the viewport's wider aspect would letterbox a second time.
///
/// # Safety
///
/// `widget` is the widget in RCX at native UpdateRenderTransform, on the game
/// thread.
pub(super) unsafe fn on_transform(widget: *mut UWidget) {
    if unsafe { &*widget }.Name != MOVIE_NAME.get() {
        return;
    }
    let layout = layout_box(widget);
    if !arranged(&layout.size) {
        return;
    }
    let fit = fit_16_9(layout.size.X, layout.size.Y);
    let next = FVector2D::new(fit.x, fit.y);
    let scale = unsafe { &mut (*widget).RenderTransform.Scale };
    let refit = !approximately(scale.X, next.X) || !approximately(scale.Y, next.Y);
    // Native UpdateRenderTransform consumes this field immediately after the
    // hook, so it is written every time even when the value is unchanged.
    *scale = next;
    if refit {
        log_report(
            "summon_movie",
            format!("Native summon image: {}", describe(&layout, fit)),
        );
    }
    refresh_movie_backdrops();
}

/// Letterboxes a playing FMV to 16:9.
///
/// Invoked by the native movie frame and texture-assignment hooks, never by a
/// timer, so the fit is re-applied exactly when the engine touches the image.
/// `object` is a `UCPP_WBP_MovieScreen` when `is_screen`, else a
/// `UCPP_WBP_MovieProjector`.
fn on_native_movie(object: *mut UObject, is_screen: bool) {
    unsafe {
        let projector = if is_screen {
            (*object.cast::<UCPP_WBP_MovieScreen>()).Projector
        } else {
            object.cast::<UCPP_WBP_MovieProjector>()
        };
        if projector.is_null() || (*projector).Projector.is_null() {
            return;
        }
        let picture = (*projector).Projector.cast::<UWidget>();
        let layout = layout_box(picture);
        if !arranged(&layout.size) {
            return;
        }
        let fit = fit_16_9(layout.size.X, layout.size.Y);
        // GetFullName walks the outer chain building a string, so the key is
        // only built on a frame that actually refits.
        if apply_center_scale(picture, fit) {
            let label = if is_screen {
                "OnFrame"
            } else {
                "SetManaTexture"
            };
            log_report(
                &format!("native_movie:{}", (*projector).GetFullName()),
                format!("Native FMV: {label}, {}", describe(&layout, fit)),
            );
        }
    }
    refresh_movie_backdrops();
}

/// Carries a wrapped movie root's visibility onto the wrapper that holds its
/// black backdrop, so the backdrop is dismissed with the movie.
///
/// A Visible movie becomes SelfHitTestInvisible on the wrapper so the overlay
/// never takes input the movie itself would not have taken.
///
/// # Safety
///
/// `widget` is the widget in RCX and `visibility` the new value in DL at native
/// `UWidget::SetVisibility`, on the game thread.
pub(super) unsafe fn on_visibility(widget: *mut UWidget, visibility: ESlateVisibility) {
    if WRAPPING.get() {
        return;
    }
    unsafe {
        let slot = (*widget).Slot;
        if slot.is_null() || (*slot).Parent.is_null() {
            return;
        }
        let parent = &mut *(*slot).Parent;
        let name = (&*widget).GetFullName();
        let wrapped = WRAPPED_ROOTS
            .with_borrow(|roots| roots.get(&name).is_some_and(|w| w.overlay == parent.Index));
        if !wrapped {
            return;
        }
        let value = mirrored_visibility(visibility);
        if parent.Visibility != value {
            parent.SetVisibility(value);
        }
    }
}

/// Wraps a movie's widget-tree root in an overlay carrying a black backdrop.
///
/// Runs from the user-widget rebuild, before the tree root is consumed for the
/// Slate rebuild, which is the one moment the root can be replaced. The overlay
/// holds the black image behind the movie and the movie above it, both filling
/// the overlay, so the bars beside a letterboxed movie are black rather than
/// the world behind it.
///
/// # Safety
///
/// `user` is the UUserWidget in RBX at the rebuild, on the game thread.
pub(super) unsafe fn on_rebuild(user: *mut UUserWidget) {
    if WRAPPING.get() {
        return;
    }
    let root = wrappable_movie_root(user);
    if root.is_null() {
        return;
    }

    let _guard = WrapGuard::new(&WRAPPING);

    unsafe {
        let tree = (*user).WidgetTree;
        let overlay = spawn::<UOverlay>(tree);
        let black = spawn_backdrop_image(tree);
        if overlay.is_null() || black.is_null() {
            log::warn!("Movie backdrop: allocation failed.");
            return;
        }
        if !fill_overlay(overlay, black.cast()) {
            log::warn!("Movie backdrop: background slot failed.");
            return;
        }
        if !fill_overlay(overlay, root) {
            log::warn!("Movie backdrop: movie slot failed.");
            return;
        }

        if waits_for_sequence(root) {
            // A new cinematic begins with its lead-in, whatever the last one said.
            MOVIE_PHASE.set(false);
        }
        (*black).SetVisibility(backdrop_visibility(root));
        (*overlay).SetVisibility(mirrored_visibility((*root).Visibility));
        (*tree).RootWidget = overlay.cast();
        let black = &*black;
        let wrapper = BackdropWrapper {
            overlay: (&*overlay).Index,
            black: black.Index,
            black_name: black.GetFullName(),
        };
        let root_name = (&*root).GetFullName();
        WRAPPED_ROOTS.with_borrow_mut(|roots| roots.insert(root_name, wrapper));
        log::info!(
            "Native movie backdrop: wrapped {}.{}",
            (*user).GetName(),
            (*root).GetName()
        );
    }
}
