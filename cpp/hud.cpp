#include "utils.hpp"

namespace ffrs {
namespace {

/// @brief The authored render translation of one tooltip, one axis each.
///
/// apply_offset re-derives the baseline from any value it did not write itself,
/// so an entry must outlive the widget's correction: dropping it while the
/// tooltip still carries the inset would treat the corrected translation as
/// authored and subtract the inset a second time.
struct Baseline {
    OffsetState x{}, y{};
};

std::unordered_map<std::string, Baseline> tooltip_baselines;

/// @brief What walking outward from a tooltip found.
struct RootSearch {
    /// A tooltip ancestor came first, so this tooltip already moves with it.
    bool nested;
    /// The constrained root, or nullptr when it is not in the chain yet.
    UWidget* root;
};

/// @brief Walks outward from a tooltip to the constrained UI root.
/// @param widget The tooltip being corrected.
/// @param tooltip_class Cached UCPP_WBP_Tooltip class, compared against each step.
/// @return Whether a tooltip ancestor came first, and the root if one was reached.
RootSearch find_constrained_root(UWidget* widget, UClass* tooltip_class) {
    const auto overall_class = UWBP_OverAllLayout_C::StaticClass();
    for (UWidget* node = outward(widget); node; node = outward(node)) {
        if (node->IsA(tooltip_class)) {
            return {true, nullptr};
        }
        if (node->Class == overall_class) {
            return {false, node};
        }
    }
    return {false, nullptr};
}

/// @brief How far the constrained root sits in from each viewport edge.
///
/// The root's own arranged box is the measurement once Slate has produced one.
/// A tooltip the game positions before attaching it has no root in its chain
/// yet, and there the rectangle the slot hook anchors the root to is the same
/// inset, so the letterbox geometry stands in for the measurement.
///
/// @param root_box The root's arranged box, or an empty box if it has none yet.
/// @param screen The viewport in local (UMG) units.
/// @param aspect The ratio the root is held inside.
/// @return The inset on each axis, in the same units as @p screen.
FVector2D tooltip_inset(const FVector2D& root_box, const FVector2D& screen, double aspect) {
    if (arranged(root_box)) {
        return {(screen.X - root_box.X) * 0.5, (screen.Y - root_box.Y) * 0.5};
    }
    const auto centered = letterbox_inset(screen.X, screen.Y, aspect);
    return {centered.x, centered.y};
}

/// @brief The baseline entry for one tooltip, created on first correction.
/// @param key Identity of the tooltip, stable across transform updates.
/// @param inset The inset about to be applied.
/// @return The entry, or nullptr when an unconstrained root would move nothing
///         and no entry exists yet to keep consistent.
Baseline* tooltip_baseline(const std::string& key, const FVector2D& inset) {
    auto entry = tooltip_baselines.find(key);
    if (entry != tooltip_baselines.end()) {
        return &entry->second;
    }
    if (inset.X < 0.5 && inset.Y < 0.5) {
        return nullptr;
    }
    return &tooltip_baselines.emplace(key, Baseline{}).first->second;
}

// A top-level screen is owned by the overall layout's widget tree. Nested
// controls have another UserWidget owner and must keep their authored bounds.
bool top_level_screen(UUserWidget* screen) {
    auto tree = screen->Outer;
    auto owner = live(tree) && tree->IsA(UWidgetTree::StaticClass()) ? tree->Outer : nullptr;
    return live(owner) && owner->Class == UWBP_OverAllLayout_C::StaticClass();
}

bool unpositioned(UWidget* widget) {
    const auto& transform = widget->RenderTransform;
    return approximately(transform.Translation.X, 0) && approximately(transform.Translation.Y, 0)
           && approximately(transform.Angle, 0) && approximately(transform.Shear.X, 0)
           && approximately(transform.Shear.Y, 0);
}

// Follow only the full-fill, backmost branch. Crossing a UserWidget's tree
// lets reusable background components satisfy exactly the same rules as an
// image embedded directly in the screen. Always scale the leaf image, never
// a container that could also carry menu controls.
UWidget* background_image(UWidget* node) {
    for (int depth = 0; depth < 32 && live(node); ++depth) {
        if (!unpositioned(node)) {
            return nullptr;
        }
        if (node->IsA(UImage::StaticClass())) {
            // UImage draws both flat dimming and artwork. Only resource-free
            // brushes are solid fills; textures, materials (including dynamic
            // instances), and named/dynamically loaded images stay authored.
            const auto& brush = static_cast<UImage*>(node)->Brush;
            return !brush.ResourceObject && brush.ResourceName.IsNone()
                           && !brush.bIsDynamicallyLoaded
                       ? node
                       : nullptr;
        }
        if (node->IsA(UUserWidget::StaticClass())) {
            auto tree = static_cast<UUserWidget*>(node)->WidgetTree;
            node = live(tree) ? tree->RootWidget : nullptr;
            continue;
        }
        if (!node->IsA(UOverlay::StaticClass())) {
            return nullptr;
        }
        const auto& slots = static_cast<UOverlay*>(node)->Slots;
        if (slots.Num() == 0 || !live(slots[0]) || !slots[0]->IsA(UOverlaySlot::StaticClass())) {
            return nullptr;
        }
        auto slot = static_cast<UOverlaySlot*>(slots[0]);
        if (slot->HorizontalAlignment != EHorizontalAlignment::HAlign_Fill
            || slot->VerticalAlignment != EVerticalAlignment::VAlign_Fill
            || !approximately(slot->Padding.Left, 0) || !approximately(slot->Padding.Right, 0)
            || !approximately(slot->Padding.Top, 0) || !approximately(slot->Padding.Bottom, 0)) {
            return nullptr;
        }
        node = slot->Content;
    }
    return nullptr;
}

UWidget* embedded_menu_backdrop(UWidget* widget) {
    if (!widget->IsA(UUserWidget::StaticClass()) && !widget->IsA(UImage::StaticClass())) {
        return nullptr;
    }
    // A reusable component's rebuild/show also reaches its owning screen.
    // This is event-driven and does not cache widgets across collection.
    auto node = widget;
    for (int depth = 0; depth < 32 && live(node); ++depth, node = outward(node)) {
        if (node->IsA(UUserWidget::StaticClass())
            && top_level_screen(static_cast<UUserWidget*>(node))) {
            auto tree = static_cast<UUserWidget*>(node)->WidgetTree;
            auto root = live(tree) ? tree->RootWidget : nullptr;
            // Require a screen containing a background and foreground content.
            // A movie whose entire root is an image is not a menu background.
            if (!live(root) || !root->IsA(UOverlay::StaticClass())
                || static_cast<UOverlay*>(root)->Slots.Num() < 2) {
                return nullptr;
            }
            auto background = background_image(root);
            return is_mod_backdrop(background) ? nullptr : background;
        }
    }
    return nullptr;
}

} // namespace

/// @brief Takes the constrained root's inset back out of a tooltip's position.
///
/// The game positions a tooltip with a render translation it computes in
/// viewport coordinates, but the widget hangs under a root whose slot anchors
/// hold it inside @p aspect, so the translation lands one root inset too far in.
/// The inset comes back out exactly once, tracked per widget by apply_offset,
/// which keeps the game's authored value as the baseline and rewrites
/// baseline+inset idempotently however often the transform updates.
///
/// Everything here is in local (UMG) units, which is what a render translation
/// is expressed in: at 7680x2160 with viewport scale 2 the viewport is 3840x1080
/// local units and the root arranges to 1920x1080, so the inset is 960,0.
///
/// @param address The widget, from RCX at native UpdateRenderTransform.
/// @param aspect The ratio the root is held inside.
/// @return A log line when the position changed, empty otherwise.
rust::String correct_tooltip_position(std::size_t address, double aspect) {
    auto widget = reinterpret_cast<UWidget*>(address);
    const auto tooltip_class = UCPP_WBP_Tooltip::StaticClass();
    // A blueprint's template widget reaches these callbacks alongside its
    // instances, and this one writes to the widget: a translation written onto
    // a template is inherited by every instance spawned from it afterwards.
    // live() rejects defaults and archetypes for that reason, not to doubt the
    // pointer the engine passed.
    if (!live(widget) || !widget->IsA(tooltip_class)) {
        return {};
    }

    const auto search = find_constrained_root(widget, tooltip_class);
    if (search.nested) {
        return {};
    }

    // The root is attached, so it resolves the viewport even while the tooltip
    // itself is being built and cannot.
    const auto screen = viewport_local_size(search.root ? search.root : widget);
    if (!arranged(screen)) {
        return {};
    }

    const auto root_box =
        search.root ? USlateBlueprintLibrary::GetLocalSize(search.root->GetCachedGeometry())
                    : FVector2D{};
    const auto inset = tooltip_inset(root_box, screen, aspect);

    const auto key = widget->GetFullName() + ":" + std::to_string(widget->Index);
    auto* baseline = tooltip_baseline(key, inset);
    if (!baseline) {
        return {};
    }

    const auto before = widget->RenderTransform.Translation;
    // Native UpdateRenderTransform consumes this field immediately after the hook.
    widget->RenderTransform.Translation = FVector2D{
        apply_offset(baseline->x, static_cast<float>(before.X), static_cast<float>(-inset.X)),
        apply_offset(baseline->y, static_cast<float>(before.Y), static_cast<float>(-inset.Y))};

    return hud_report("tooltip_position:" + key,
                      std::format("Tooltip position: {}, root_box={:g}x{:g}{}, screen={:g}x{:g}, "
                                  "inset={:g},{:g}, authored={:g},{:g}, applied={:g},{:g}",
                                  widget->GetName(), root_box.X, root_box.Y,
                                  search.root ? "" : " (root not in chain)", screen.X, screen.Y,
                                  inset.X, inset.Y, baseline->x.baseline, baseline->y.baseline,
                                  widget->RenderTransform.Translation.X,
                                  widget->RenderTransform.Translation.Y));
}

/// @brief Scales a menu's dimming image across the whole viewport.
///
/// A menu dims the scene behind it with a full-bleed image in a WBP_BGDimming
/// child widget, one per menu layout, all of them under the HUD root: logs
/// record it inside WBP_SystemMenuLayout_C, WBP_OptionLayout_C,
/// WBP_DialogLayout_C, WBP_LoadLayout_C and WBP_TutorialLayout_C. The root's
/// inset leaves the letterbox columns undimmed, so the dimming widget is
/// render-scaled about its center -- a 16:9 root on a 32:9 screen by 2,1.
/// Layout, hit testing and every sibling stay exactly as authored.
///
/// Backgrounds are recognized by their full-fill position behind the screen
/// controls, including across reusable widget trees, without widget-name rules.
///
/// @param address The widget, from RCX at SetVisibility or the widget rebuild.
/// @return A log line when the scale changed, empty otherwise.
rust::String expand_menu_backdrop(std::size_t address) {
    auto widget = reinterpret_cast<UWidget*>(address);
    // The blueprint's own WBP_BGDimming template arrives here as well as each
    // instance, and expand_backdrop writes a render scale: scaling the template
    // would hand that scale to every menu spawned later.
    if (!live(widget)) {
        return {};
    }
    widget = embedded_menu_backdrop(widget);
    if (!widget) {
        return {};
    }
    // On rebuild the widget has no geometry yet, and the walk inside
    // expand_backdrop reaches the root: the dimming image fills the root's box
    // once arranged, and the root's player context resolves the viewport the
    // widget itself cannot yet report.
    const auto name = widget->GetFullName();
    return expand_backdrop(widget, "menu_backdrop:" + name, "Menu backdrop: " + name);
}

/// @brief Holds a viewport root inside a centered rectangle of @p aspect.
///
/// Anchors are ratios of the viewport, so measuring it in local units gives the
/// identical rectangle at any resolution or DPI, and records the earliest
/// viewport every later correction can fall back on.
///
/// @param widget_addr The root widget, from the register each hooked path uses.
/// @param slot_addr Its FGameViewportWidgetSlot, written before the game reads it.
/// @param path Which of the three hooked paths called, for the log.
/// @param aspect The ratio to hold the root inside, overridden for the encounter layout.
/// @return A log line when the anchors changed, empty otherwise.
rust::String constrain_hud_slot(std::size_t widget_addr, std::size_t slot_addr, std::uint32_t path,
                                double aspect) {
    auto widget = reinterpret_cast<UObject*>(widget_addr);
    // Path 1 hooks the slot update at its entry, and that function opens with
    // TEST RDX,RDX / JZ -- the first bytes of its own signature. The engine
    // calls it with a null widget, so the hook sees one before the engine's
    // test does.
    if (!live(widget)) {
        return {};
    }
    // The encounter transition draws the captured frame as a picture of the
    // capture's own shape, and encounter.cpp re-frames that frame to be one, so
    // this layout is held to the capture's shape rather than the HUD's. Its
    // Blueprint class is loaded on demand and absent from the static SDK, so it
    // is recognised by the name the live class carries.
    static const auto encounter = UKismetStringLibrary::Conv_StringToName(L"WBP_EncountLayout_C");
    if (widget->Class->Name == encounter) {
        aspect = captured_aspect();
    } else if (widget->Class != UWBP_OverAllLayout_C::StaticClass()) {
        return {};
    }
    const auto viewport = viewport_local_size(widget);
    if (!arranged(viewport)) {
        return {};
    }

    const auto inset = letterbox_inset(viewport.X, viewport.Y, aspect);
    auto& slot = *reinterpret_cast<FGameViewportWidgetSlot*>(slot_addr);
    slot.Anchors.Minimum = FVector2D{inset.x / viewport.X, inset.y / viewport.Y};
    slot.Anchors.Maximum = FVector2D{1.0 - slot.Anchors.Minimum.X, 1.0 - slot.Anchors.Minimum.Y};
    slot.Offsets = FMargin{};
    slot.Alignment = FVector2D{};

    return hud_report(
        "native_hud:" + widget->Class->GetName() + ":" + std::to_string(path),
        std::format("Native HUD slot: path={}, root={}, aspect={:g}, screen={:g}x{:g}, "
                    "anchors={:g},{:g}/{:g},{:g}",
                    path, widget->GetName(), aspect, viewport.X, viewport.Y, slot.Anchors.Minimum.X,
                    slot.Anchors.Minimum.Y, slot.Anchors.Maximum.X, slot.Anchors.Maximum.Y));
}
} // namespace ffrs
