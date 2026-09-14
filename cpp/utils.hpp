#pragma once
#include "WBP_OverAllLayout_classes.hpp"
#include "bridge.hpp"
#include "final-fantasy-resonance-fix/src/bridge.rs.h"
#include <algorithm>
#include <cmath>
#include <format>
#include <string>
#include <unordered_map>
#include <unordered_set>

/// @file
/// Shared helpers every correction is built from: object liveness, the geometry
/// each one measures against, and the log de-duplication they all report
/// through. This header doubles as the prelude, so it pulls in the SDK and the
/// generated cxx bridge for the .cpp files that include it.

namespace ffrs {
using namespace SDK;

/// @brief Whether an object is safe to read and safe to write to.
///
/// Rejects null, an object with no class, and -- because every caller writes to
/// what it is given -- class defaults, archetypes and objects being destroyed.
/// A blueprint's template widget reaches the hooked callbacks alongside its
/// instances, and a value written onto a template is inherited by every
/// instance spawned from it afterwards.
bool live(UObject* object);

/// @brief Whether two values are equal within the tolerance used for geometry.
bool approximately(double a, double b);

/// @brief Whether a size is one a correction can measure against.
///
/// Finite, and at least one unit on both axes. Slate reports an empty box for a
/// widget it has not arranged yet, and the viewport query reports 1,1 when it
/// resolves no world.
bool arranged(const FVector2D& size);

/// @brief De-duplicates a log line against the last one under the same key.
/// @param key Identity of the correction reporting.
/// @param text The line it would write.
/// @return @p text when it differs from the last under @p key, empty otherwise,
///         so a correction that runs every frame logs when it changes rather
///         than per frame.
std::string hud_report(const std::string& key, const std::string& text);

/// @brief One step outward through the UMG hierarchy.
///
/// UWidget::Slot is null for a widget-tree root, and there the tree's outer
/// UUserWidget carries the slot in whatever panel displays it, so the walk
/// crosses blueprint boundaries.
///
/// @return The next widget outward, or nullptr at the top of the chain. Never
///         returns an object that is not live().
UWidget* outward(UWidget* node);

/// @brief The box a widget is arranged into and the viewport it sits in.
struct LayoutBox {
    /// The box Slate arranged, or will arrange, the widget into.
    FVector2D size;
    /// The viewport in the same units, measured from the same widget.
    FVector2D screen;
    /// Which widget @ref size came from: "self", a class name, or "viewport".
    std::string source;
};

/// @brief Measures the box a widget is arranged into, in local (UMG) units.
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
/// other layout sizes. That is not the same space as GetViewportSize, which
/// reports physical pixels: at 7680x2160 with viewport scale 2 the whole
/// viewport is 3840x1080 local units, and a root held inside 16:9 arranges to
/// 1920x1080.
LayoutBox layout_box(UWidget* widget);

/// @brief The viewport in local (UMG) units.
///
/// UWidgetLayoutLibrary resolves a world from the context object and reports
/// the viewport as 1,1 when it resolves none, which is what a widget still
/// being built reports, so a viewport that small means unavailable rather than
/// a one-pixel screen. A measurement that does resolve is kept and answers the
/// ones that do not: the HUD root's viewport attachment measures it before any
/// menu, movie or tooltip exists, and every later resolving call refreshes it,
/// so a resolution change is picked up from whichever correction runs next.
///
/// @param context Any object with a resolvable world; an attached widget has one.
/// @return The viewport, or zero until the first measurement resolves, which
///         leaves callers with nothing to compute against and skipping.
FVector2D viewport_local_size(UObject* context);

/// @brief Formats what a correction measured and what it wrote, for a log line.
std::string describe(const LayoutBox& box, const Scale& scale);

/// @brief Writes a render scale about a widget's center.
///
/// The pivot is only written when it is not already centered, since both fields
/// are pushed through native setters that rebuild the widget's render transform.
///
/// @return Whether the scale itself changed.
bool apply_center_scale(UWidget* widget, const Scale& scale);

/// @brief Paints a backdrop widget across the whole viewport.
///
/// Slate clips nothing by default (EWidgetClipping::Inherit), so a render scale
/// reaches past the arranged box to the screen edges, while layout, hit testing
/// and every sibling stay as authored. A widget already filling the viewport,
/// or one the game sized short of it on both axes, is left alone by
/// full_bleed_scale.
///
/// @param key Identity for log de-duplication.
/// @param label Prefix for the log line.
/// @return A log line when the scale changed, empty otherwise.
std::string expand_backdrop(UWidget* widget, const std::string& key, const std::string& label);

/// @brief Spawns the opaque black image that sits behind a wrapped widget.
///
/// Every image spawned here is remembered, so @ref is_mod_backdrop can tell
/// one of ours from a background the game authored.
///
/// @return The image, hit-test invisible so it never takes input, or nullptr if
///         the allocation failed.
UImage* spawn_backdrop_image(UWidgetTree* tree);

/// @brief The shape of the frame the encounter transition draws.
///
/// Defined in encounter.cpp, which sees the capture's target, and called from
/// the HUD constraint that holds the transition's layout to that shape.
double captured_aspect();

/// @brief Whether this widget is a backdrop this mod spawned.
///
/// A wrapped tree has the shape a menu screen does -- a root overlay whose
/// first filling child is a resource-free image -- so a fix that searches for
/// menu dimming has to skip the backdrops another fix already owns, rather
/// than writing a render scale to a widget it does not control.
bool is_mod_backdrop(const UWidget* widget);

/// @brief Adds a child that fills an overlay on both axes.
/// @return Whether the overlay gave the child a slot.
bool fill_overlay(UOverlay* overlay, UWidget* child);

/// @brief The visibility a wrapper takes from the widget it wraps.
///
/// Visible becomes SelfHitTestInvisible, so a wrapper never takes input the
/// widget inside it would not have taken.
ESlateVisibility mirrored_visibility(ESlateVisibility value);

/// @brief A widget-tree root this mod wrapped, held as identities only.
///
/// The widgets are re-derived from these rather than stored, so a collected and
/// reused object index is detected rather than dereferenced.
struct BackdropWrapper {
    int overlay;            ///< The overlay that replaced the tree root.
    int black;              ///< The black image behind the root's content.
    std::string black_name; ///< Full name @ref black had when it was recorded.
};

/// @brief The backdrop a wrapper recorded, if that index still refers to it.
UImage* live_backdrop(const BackdropWrapper& wrapper);

/// @brief Holds a flag for as long as a wrap is spawning widgets.
///
/// Spawning and reparenting trigger visibility changes of their own, and the
/// callback they arrive at must not mistake them for the game dismissing what
/// is being wrapped.
class WrapGuard {
    bool& flag;

public:
    explicit WrapGuard(bool& flag) : flag(flag) {
        flag = true;
    }

    ~WrapGuard() {
        flag = false;
    }

    WrapGuard(const WrapGuard&) = delete;
    WrapGuard& operator=(const WrapGuard&) = delete;
};

} // namespace ffrs
