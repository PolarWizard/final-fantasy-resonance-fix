#include "utils.hpp"

namespace ffrs {
namespace {

/// The last viewport a context object with a resolvable world reported, in
/// local units. Read and written on the game thread, the only one UMG runs on.
FVector2D measured_viewport{};

} // namespace

bool live(UObject* object) {
    return object && object->Class && !object->IsDefaultObject()
           && !(object->Flags
                & (EObjectFlags::BeginDestroyed | EObjectFlags::FinishDestroyed
                   | EObjectFlags::ArchetypeObject));
}

bool approximately(double a, double b) {
    return std::abs(a - b) < 0.00001;
}

bool arranged(const FVector2D& size) {
    return std::isfinite(size.X) && std::isfinite(size.Y) && size.X >= 1 && size.Y >= 1;
}

std::string hud_report(const std::string& key, const std::string& text) {
    static std::unordered_map<std::string, std::string> previous;
    if (previous.size() >= 128) {
        previous.clear();
    }
    auto [entry, inserted] = previous.try_emplace(key, text);
    if (!inserted && entry->second == text) {
        return {};
    }
    entry->second = text;
    return text + "\n";
}

UWidget* outward(UWidget* node) {
    if (live(node->Slot) && live(node->Slot->Parent)) {
        return node->Slot->Parent;
    }
    auto tree = node->Outer; // UWidgetTree, for the root widget it owns.
    auto owner = live(tree) ? tree->Outer : nullptr;
    return live(owner) && owner->IsA(UUserWidget::StaticClass()) ? static_cast<UWidget*>(owner)
                                                                 : nullptr;
}

LayoutBox layout_box(UWidget* widget) {
    UWidget* node = widget;
    for (int step = 0; step < 32 && live(node); ++step, node = outward(node)) {
        const auto size = USlateBlueprintLibrary::GetLocalSize(node->GetCachedGeometry());
        // The viewport is read from the widget the geometry came from, not from
        // the one asked about: an arranged widget is attached and has the
        // player context the viewport query needs, while a widget still being
        // built does not and would report the viewport as unavailable.
        if (arranged(size)) {
            return {size, viewport_local_size(node),
                    node == widget ? std::string("self") : node->Class->GetName()};
        }
    }
    const auto local = viewport_local_size(widget);
    return {local, local, "viewport"};
}

FVector2D viewport_local_size(UObject* context) {
    const auto viewport = UWidgetLayoutLibrary::GetViewportSize(context);
    const auto dpi = UWidgetLayoutLibrary::GetViewportScale(context);
    // GetViewportSize reports physical pixels; dividing by the viewport scale
    // puts the result in the local units every arranged box is measured in.
    if (arranged(viewport) && viewport.X > 1 && viewport.Y > 1 && std::isfinite(dpi) && dpi > 0) {
        measured_viewport = FVector2D{viewport.X / dpi, viewport.Y / dpi};
    }
    return measured_viewport;
}

bool apply_center_scale(UWidget* widget, const Scale& scale) {
    if (!approximately(widget->RenderTransformPivot.X, 0.5)
        || !approximately(widget->RenderTransformPivot.Y, 0.5)) {
        widget->SetRenderTransformPivot(FVector2D{0.5, 0.5});
    }
    if (approximately(widget->RenderTransform.Scale.X, scale.x)
        && approximately(widget->RenderTransform.Scale.Y, scale.y)) {
        return false;
    }
    widget->SetRenderScale(FVector2D{scale.x, scale.y});
    return true;
}

std::string expand_backdrop(UWidget* widget, const std::string& key, const std::string& label) {
    // Both sides of the comparison and the ratio are local units.
    const auto box = layout_box(widget);
    if (!arranged(box.size) || !arranged(box.screen)) {
        return {};
    }
    const auto scale = full_bleed_scale(box.size.X, box.size.Y, box.screen.X, box.screen.Y);
    // Nothing was applied, so there is nothing to report: skipping here keeps a
    // steady-state frame from formatting a line for hud_report to discard.
    if (!apply_center_scale(widget, scale)) {
        return {};
    }
    return hud_report(key, label + ": " + describe(box, scale));
}

std::string describe(const LayoutBox& box, const Scale& scale) {
    return std::format("box={:g}x{:g} ({}), screen={:g}x{:g}, scale={:g},{:g}", box.size.X,
                       box.size.Y, box.source, box.screen.X, box.screen.Y, scale.x, scale.y);
}

} // namespace ffrs
