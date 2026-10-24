//! Shared helpers every correction is built from: object liveness, the geometry
//! each one measures against, and the backdrops a movie or the map is wrapped
//! in.
//!
//! Every pointer reaching these helpers comes from a hook's registers or from
//! the game's own object graph (slots, outers, widget trees, GObjects), and is
//! only read on the game thread UMG runs on, the same contract the SDK's own
//! `rt` helpers work under.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ptr::null_mut,
    thread::LocalKey,
};

use sdk::{
    CoreUObject::{FLinearColor, FVector2D, UObject},
    Engine::UGameplayStatics,
    SlateCore::{EHorizontalAlignment, ESlateBrushDrawType, EVerticalAlignment},
    UMG::{
        ESlateVisibility, UImage, UOverlay, USlateBlueprintLibrary, UUserWidget, UWidget,
        UWidgetLayoutLibrary, UWidgetTree,
    },
    rt::{EObjectFlags, FName, TSubclassOf, UClassType},
};

use super::layout::{Scale, full_bleed_scale};
use crate::utils::log_report;

thread_local! {
    /// The last viewport a context object with a resolvable world reported, in
    /// local units.
    static MEASURED_VIEWPORT: Cell<FVector2D> = const { Cell::new(FVector2D { X: 0.0, Y: 0.0 }) };

    /// Object index and name of every backdrop [`spawn_backdrop_image`] made.
    /// An index a collected backdrop left behind can be reused by an authored
    /// image, which must not be mistaken for ours, and the name tells them apart.
    static SPAWNED_BACKDROPS: RefCell<HashMap<i32, FName>> = RefCell::new(HashMap::new());
}

/// Whether an object is safe to read and safe to write to.
///
/// Rejects null, an object with no class, and -- because every caller writes to
/// what it is given -- class defaults, archetypes and objects being destroyed.
/// A blueprint's template widget reaches the hooked callbacks alongside its
/// instances, and a value written onto a template is inherited by every
/// instance spawned from it afterwards.
///
/// It vouches only for a pointer the engine handed in during the current call,
/// or one reached from such a pointer in the same call. It reads the object's
/// own fields, so a freed object whose memory still holds its old values
/// passes. Anything kept across calls is kept as an object index and name and
/// resolved through GObjects each time (see [`BackdropWrapper`]), because the
/// engine clears an object's slot when it frees the object.
pub fn live<T>(object: *const T) -> bool {
    let object = object.cast::<UObject>();
    if object.is_null() {
        return false;
    }
    let object = unsafe { &*object };
    let dead = EObjectFlags::BeginDestroyed
        | EObjectFlags::FinishDestroyed
        | EObjectFlags::ArchetypeObject;
    !object.Class.is_null() && !object.IsDefaultObject() && (object.Flags & dead).0 == 0
}

/// Whether two values are equal within the tolerance used for geometry.
pub fn approximately(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.00001
}

/// Whether a size is one a correction can measure against.
///
/// Finite, and at least one unit on both axes. Slate reports an empty box for a
/// widget it has not arranged yet, and the viewport query reports 1,1 when it
/// resolves no world.
pub fn arranged(size: &FVector2D) -> bool {
    size.X.is_finite() && size.Y.is_finite() && size.X >= 1.0 && size.Y >= 1.0
}

/// One step outward through the UMG hierarchy.
///
/// `UWidget::Slot` is null for a widget-tree root, and there the tree's outer
/// `UUserWidget` carries the slot in whatever panel displays it, so the walk
/// crosses blueprint boundaries.
///
/// Returns the next widget outward, or null at the top of the chain. Never
/// returns an object that is not [`live`].
pub fn outward(node: *mut UWidget) -> *mut UWidget {
    let node = unsafe { &*node };
    unsafe {
        if live(node.Slot) && live((*node.Slot).Parent) {
            return (*node.Slot).Parent.cast();
        }
        let tree = node.Outer; // UWidgetTree, for the root widget it owns.
        let owner = if live(tree) {
            (*tree).Outer
        } else {
            null_mut()
        };
        if live(owner) && (*owner).IsA(UUserWidget::StaticClass()) {
            owner.cast()
        } else {
            null_mut()
        }
    }
}

/// The box a widget is arranged into and the viewport it sits in.
pub struct LayoutBox {
    /// The box Slate arranged, or will arrange, the widget into.
    pub size: FVector2D,
    /// The viewport in the same units, measured from the same widget.
    pub screen: FVector2D,
    /// Which widget `size` came from: "self", a class name, or "viewport".
    pub source: String,
}

/// Measures the box a widget is arranged into, in local (UMG) units.
///
/// Cached geometry carries the arranged (layout) size, which a render transform
/// on the widget or an ancestor does not feed back into, so a scale derived
/// from it can be assigned absolutely without accumulating across calls.
///
/// Before Slate has arranged a widget its cached geometry is empty, so the walk
/// continues outward: a movie image fills its parents, so the nearest ancestor
/// already on screen has the box this widget is about to receive. When nothing
/// in the chain is arranged -- a widget attached straight to the viewport, on
/// the frame it appears -- the viewport itself is the box.
///
/// The size is always in local (UMG) units, so callers can compare it against
/// other layout sizes. That is not the same space as `GetViewportSize`, which
/// reports physical pixels: at 7680x2160 with viewport scale 2 the whole
/// viewport is 3840x1080 local units, and a root held inside 16:9 arranges to
/// 1920x1080.
pub fn layout_box(widget: *mut UWidget) -> LayoutBox {
    let mut node = widget;
    for _ in 0..32 {
        if !live(node) {
            break;
        }
        let current = unsafe { &mut *node };
        let size = USlateBlueprintLibrary::GetLocalSize(&current.GetCachedGeometry());
        // The viewport is read from the widget the geometry came from, not from
        // the one asked about: an arranged widget is attached and has the
        // player context the viewport query needs, while a widget still being
        // built does not and would report the viewport as unavailable.
        if arranged(&size) {
            let source = if node == widget {
                "self".to_owned()
            } else {
                unsafe { &*current.Class }.GetName()
            };
            return LayoutBox {
                size,
                screen: viewport_local_size(node.cast()),
                source,
            };
        }
        node = outward(node);
    }
    let local = viewport_local_size(widget.cast());
    LayoutBox {
        size: local,
        screen: local,
        source: "viewport".to_owned(),
    }
}

/// The viewport in local (UMG) units.
///
/// `UWidgetLayoutLibrary` resolves a world from the context object and reports
/// the viewport as 1,1 when it resolves none, which is what a widget still
/// being built reports, so a viewport that small means unavailable rather than
/// a one-pixel screen. A measurement that does resolve is kept and answers the
/// ones that do not: the HUD root's viewport attachment measures it before any
/// menu, movie or tooltip exists, and every later resolving call refreshes it,
/// so a resolution change is picked up from whichever correction runs next.
///
/// `context` is any object with a resolvable world; an attached widget has one.
/// Returns the viewport, or zero until the first measurement resolves, which
/// leaves callers with nothing to compute against and skipping.
pub fn viewport_local_size(context: *mut UObject) -> FVector2D {
    let viewport = UWidgetLayoutLibrary::GetViewportSize(context);
    let dpi = f64::from(UWidgetLayoutLibrary::GetViewportScale(context));
    // GetViewportSize reports physical pixels; dividing by the viewport scale
    // puts the result in the local units every arranged box is measured in.
    if arranged(&viewport) && viewport.X > 1.0 && viewport.Y > 1.0 && dpi.is_finite() && dpi > 0.0 {
        MEASURED_VIEWPORT.set(FVector2D::new(viewport.X / dpi, viewport.Y / dpi));
    }
    MEASURED_VIEWPORT.get()
}

/// Formats what a correction measured and what it wrote, for a log line.
pub fn describe(layout: &LayoutBox, scale: Scale) -> String {
    format!(
        "box={}x{} ({}), screen={}x{}, scale={},{}",
        layout.size.X,
        layout.size.Y,
        layout.source,
        layout.screen.X,
        layout.screen.Y,
        scale.x,
        scale.y
    )
}

/// Writes a render scale about a widget's center.
///
/// The pivot is only written when it is not already centered, since both fields
/// are pushed through native setters that rebuild the widget's render transform.
///
/// Returns whether the scale itself changed.
pub fn apply_center_scale(widget: *mut UWidget, scale: Scale) -> bool {
    unsafe {
        let pivot = (*widget).RenderTransformPivot;
        if !approximately(pivot.X, 0.5) || !approximately(pivot.Y, 0.5) {
            (*widget).SetRenderTransformPivot(&FVector2D::new(0.5, 0.5));
        }
        let current = (*widget).RenderTransform.Scale;
        if approximately(current.X, scale.x) && approximately(current.Y, scale.y) {
            return false;
        }
        (*widget).SetRenderScale(&FVector2D::new(scale.x, scale.y));
    }
    true
}

/// Paints a backdrop widget across the whole viewport, logging under `key`
/// with `label` when the scale changed.
///
/// Slate clips nothing by default (`EWidgetClipping::Inherit`), so a render
/// scale reaches past the arranged box to the screen edges, while layout, hit
/// testing and every sibling stay as authored. A widget already filling the
/// viewport, or one the game sized short of it on both axes, is left alone by
/// [`full_bleed_scale`].
pub fn expand_backdrop(widget: *mut UWidget, key: &str, label: &str) {
    // Both sides of the comparison and the ratio are local units.
    let layout = layout_box(widget);
    if !arranged(&layout.size) || !arranged(&layout.screen) {
        return;
    }
    let scale = full_bleed_scale(
        layout.size.X,
        layout.size.Y,
        layout.screen.X,
        layout.screen.Y,
    );
    // Nothing was applied, so there is nothing to report: skipping here keeps a
    // steady-state frame from formatting a line the de-duplication discards.
    if apply_center_scale(widget, scale) {
        log_report(key, format!("{label}: {}", describe(&layout, scale)));
    }
}

/// Spawns a `T` owned by `outer`, or null if the allocation failed.
pub fn spawn<T: UClassType>(outer: *mut UWidgetTree) -> *mut T {
    UGameplayStatics::SpawnObject(TSubclassOf::new(T::static_class()), outer.cast()).cast()
}

/// Spawns the opaque black image that sits behind a wrapped widget.
///
/// Every image spawned here is remembered, so [`is_mod_backdrop`] can tell one
/// of ours from a background the game authored.
///
/// Returns the image, hit-test invisible so it never takes input, or null if
/// the allocation failed.
pub fn spawn_backdrop_image(tree: *mut UWidgetTree) -> *mut UImage {
    let black = spawn::<UImage>(tree);
    if black.is_null() {
        return black;
    }
    let image = unsafe { &mut *black };
    let mut brush = image.Brush;
    brush.DrawAs = ESlateBrushDrawType::Image;
    image.SetBrush(&brush);
    image.SetColorAndOpacity(&FLinearColor {
        R: 0.0,
        G: 0.0,
        B: 0.0,
        A: 1.0,
    });
    image.SetVisibility(ESlateVisibility::HitTestInvisible);
    SPAWNED_BACKDROPS.with_borrow_mut(|spawned| spawned.insert(image.Index, image.Name));
    black
}

/// Whether this widget is a backdrop this mod spawned.
///
/// A wrapped tree has the shape a menu screen does -- a root overlay whose
/// first filling child is a resource-free image -- so a fix that searches for
/// menu dimming has to skip the backdrops another fix already owns, rather
/// than writing a render scale to a widget it does not control.
pub fn is_mod_backdrop(widget: *const UWidget) -> bool {
    !widget.is_null() && {
        let widget = unsafe { &*widget };
        SPAWNED_BACKDROPS.with_borrow(|spawned| spawned.get(&widget.Index) == Some(&widget.Name))
    }
}

/// Adds a child that fills an overlay on both axes. Returns whether the
/// overlay gave the child a slot.
pub fn fill_overlay(overlay: *mut UOverlay, child: *mut UWidget) -> bool {
    unsafe {
        let slot = (*overlay).AddChildToOverlay(child);
        if slot.is_null() {
            return false;
        }
        (*slot).SetHorizontalAlignment(EHorizontalAlignment::HAlign_Fill);
        (*slot).SetVerticalAlignment(EVerticalAlignment::VAlign_Fill);
    }
    true
}

/// The visibility a wrapper takes from the widget it wraps.
///
/// Visible becomes SelfHitTestInvisible, so a wrapper never takes input the
/// widget inside it would not have taken.
pub fn mirrored_visibility(value: ESlateVisibility) -> ESlateVisibility {
    if value == ESlateVisibility::Visible {
        ESlateVisibility::SelfHitTestInvisible
    } else {
        value
    }
}

/// A widget-tree root this mod wrapped, held as identities only.
///
/// The widgets are re-derived from these rather than stored, so a collected and
/// reused object index is detected rather than dereferenced.
#[derive(Clone)]
pub struct BackdropWrapper {
    /// The overlay that replaced the tree root.
    pub overlay: i32,
    /// The black image behind the root's content.
    pub black: i32,
    /// Full name `black` had when it was recorded.
    pub black_name: String,
}

/// The backdrop a wrapper recorded, if that index still refers to it.
pub fn live_backdrop(wrapper: &BackdropWrapper) -> *mut UImage {
    let object = UObject::GObjects.GetByIndex(wrapper.black);
    if live(object) && unsafe { (*object).GetFullName() } == wrapper.black_name {
        object.cast()
    } else {
        null_mut()
    }
}

/// Holds a flag for as long as a wrap is spawning widgets.
///
/// Spawning and reparenting trigger visibility changes of their own, and the
/// callback they arrive at must not mistake them for the game dismissing what
/// is being wrapped.
pub struct WrapGuard(&'static LocalKey<Cell<bool>>);

impl WrapGuard {
    pub fn new(flag: &'static LocalKey<Cell<bool>>) -> Self {
        flag.set(true);
        Self(flag)
    }
}

impl Drop for WrapGuard {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
