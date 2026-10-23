//! The world map's black backdrop: wrapped behind the map, dismissed with it,
//! and stretched to the viewport.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ptr::null_mut,
};

use sdk::{
    SlateCore::EWidgetClipping,
    UMG::{ESlateVisibility, UImage, UOverlay, UUserWidget, UWidget},
    rt::NameCache,
};

use super::widgets::{
    BackdropWrapper, WrapGuard, expand_backdrop, fill_overlay, live, live_backdrop,
    mirrored_visibility, spawn, spawn_backdrop_image,
};

thread_local! {
    /// Every map layout this mod has wrapped, by the wrapped content's full name.
    static MAP_WRAPPERS: RefCell<HashMap<String, BackdropWrapper>> = RefCell::new(HashMap::new());

    /// Set while the wrap below spawns widgets, so the visibility changes that
    /// spawning triggers are not mistaken for the game showing the map.
    static WRAPPING_MAP: Cell<bool> = const { Cell::new(false) };
}

/// The backdrop a wrapper recorded, if it still hangs in that wrapper.
///
/// A rebuilt tree drops our overlay, which can leave the image live but
/// parented elsewhere, and an index alone can be reused by an unrelated
/// object. Only an image that resolves and still sits in the recorded overlay
/// is one of ours.
fn wrapped_backdrop(wrapper: &BackdropWrapper) -> *mut UImage {
    let black = live_backdrop(wrapper);
    if black.is_null() {
        return black;
    }
    let slot = unsafe { &*black }.Slot;
    let parent = if live(slot) {
        unsafe { (*slot).Parent }
    } else {
        null_mut()
    };
    if live(parent) && unsafe { &*parent }.Index == wrapper.overlay {
        black
    } else {
        null_mut()
    }
}

/// Every recorded wrapper, copied out so the game calls made while walking
/// them can re-enter the hooks that read the registry.
fn wrappers() -> Vec<(String, BackdropWrapper)> {
    MAP_WRAPPERS.with_borrow(|wrappers| {
        wrappers
            .iter()
            .map(|(name, wrapper)| (name.clone(), wrapper.clone()))
            .collect()
    })
}

/// Whether this widget-tree root is an overlay this fix already added.
///
/// The root is replaced by that overlay, and a replaced root has no slot
/// either, so the absence of a slot cannot tell a fresh tree from a wrapped
/// one. The recorded wrapper can.
fn already_wrapped(root: *const UWidget) -> bool {
    let index = unsafe { &*root }.Index;
    wrappers()
        .iter()
        .any(|(_, wrapper)| wrapper.overlay == index && !wrapped_backdrop(wrapper).is_null())
}

/// Wraps the world map's widget-tree root in an overlay carrying a black
/// backdrop, and clips the map to its own box.
///
/// The map content draws past its arranged box, so on a screen wider than the
/// box the map itself would fill the sides. Clipping it and painting black
/// behind it leaves the sides black instead. Both widgets live inside the map's
/// own tree, which ties their lifetime and their parent's visibility to the
/// map rather than to a viewport-level overlay of our own.
fn wrap_map_layout(widget: *mut UWidget) {
    static MAP_CLASS: NameCache = NameCache::new("WBP_WldMapLayout_C");
    unsafe {
        let map = &*widget;
        if (&*map.Class).Name != MAP_CLASS.get() || !map.IsA(UUserWidget::StaticClass()) {
            return;
        }
        let tree = (*widget.cast::<UUserWidget>()).WidgetTree;
        let root = if live(tree) {
            (*tree).RootWidget
        } else {
            null_mut()
        };
        // Only wrap a widget-tree root, never reparent a content child.
        if !live(root) || !(*root).Slot.is_null() || already_wrapped(root) {
            return;
        }

        let _guard = WrapGuard::new(&WRAPPING_MAP);

        let overlay = spawn::<UOverlay>(tree);
        let black = spawn_backdrop_image(tree);
        if overlay.is_null() || black.is_null() {
            log::warn!("Map backdrop: allocation failed.");
            return;
        }
        if !fill_overlay(overlay, black.cast()) || !fill_overlay(overlay, root) {
            log::warn!("Map backdrop: slot failed.");
            return;
        }
        (*root).SetClipping(EWidgetClipping::ClipToBoundsAlways);
        (*overlay).SetVisibility(mirrored_visibility((*root).Visibility));
        (*tree).RootWidget = overlay.cast();
        let black = &*black;
        let wrapper = BackdropWrapper {
            overlay: (&*overlay).Index,
            black: black.Index,
            black_name: black.GetFullName(),
        };
        let root_name = (&*root).GetFullName();
        MAP_WRAPPERS.with_borrow_mut(|wrappers| wrappers.insert(root_name, wrapper));
        log::info!("Map backdrop: wrapped and clipped {}", map.GetFullName());
    }
}

/// Carries a wrapped map's visibility onto the overlay holding its backdrop, so
/// the backdrop is dismissed with the map.
fn mirror_map_visibility(widget: *mut UWidget, visibility: ESlateVisibility) {
    unsafe {
        let name = (*widget).GetFullName();
        let slot = (*widget).Slot;
        let parent = if live(slot) {
            (*slot).Parent
        } else {
            null_mut()
        };
        if !live(parent) {
            return;
        }
        let parent = &mut *parent;
        let wrapped = MAP_WRAPPERS.with_borrow(|wrappers| {
            wrappers
                .get(&name)
                .is_some_and(|w| w.overlay == parent.Index)
        });
        if !wrapped {
            return;
        }
        let value = mirrored_visibility(visibility);
        if parent.Visibility != value {
            parent.SetVisibility(value);
        }
    }
}

/// Re-expands every backdrop this fix has wrapped.
///
/// The backdrop fills the map's own box, so it is stretched out to the viewport
/// the same way a movie's is. The expansion is absolute, so running it from
/// either callback converges rather than accumulating.
fn refresh_map_backdrops() {
    for (content_name, wrapper) in wrappers() {
        let black = wrapped_backdrop(&wrapper);
        if black.is_null() {
            continue;
        }
        expand_backdrop(
            black.cast(),
            &format!("map_backdrop:{}", wrapper.black_name),
            &format!("Map backdrop: {content_name}"),
        );
    }
}

/// Keeps the world map's black backdrop wrapped, dismissed and stretched.
///
/// Driven by the two shared widget callbacks rather than a timer: the rebuild
/// that runs before a tree root is consumed, which is the one moment the root
/// can be replaced, and every visibility change, which is what dismisses the
/// wrapper with the map. `visibility` is the new value at SetVisibility, and
/// `None` at the rebuild.
///
/// # Safety
///
/// `widget` is the widget in RBX at the rebuild or RCX at SetVisibility, on
/// the game thread.
pub(super) unsafe fn update_backdrop(widget: *mut UWidget, visibility: Option<ESlateVisibility>) {
    if WRAPPING_MAP.get() || !live(widget) {
        return;
    }
    match visibility {
        None => wrap_map_layout(widget),
        Some(visibility) => mirror_map_visibility(widget, visibility),
    }
    refresh_map_backdrops();
}
