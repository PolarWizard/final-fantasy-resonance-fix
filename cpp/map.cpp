#include "utils.hpp"

namespace ffrs {
namespace {

/// Every map layout this mod has wrapped, by the wrapped content's full name.
std::unordered_map<std::string, BackdropWrapper> map_wrappers;

/// Set while the wrap below spawns widgets, so the visibility changes that
/// spawning triggers are not mistaken for the game showing the map.
thread_local bool wrapping_map = false;

/// @brief The backdrop a wrapper recorded, if it still hangs in that wrapper.
///
/// A rebuilt tree drops our overlay, which can leave the image live but
/// parented elsewhere, and an index alone can be reused by an unrelated
/// object. Only an image that resolves and still sits in the recorded overlay
/// is one of ours.
UImage* wrapped_backdrop(const BackdropWrapper& wrapper) {
    auto black = live_backdrop(wrapper);
    auto parent = black && live(black->Slot) ? black->Slot->Parent : nullptr;
    return live(parent) && parent->Index == wrapper.overlay ? black : nullptr;
}

/// @brief Whether this widget-tree root is an overlay this fix already added.
///
/// The root is replaced by that overlay, and a replaced root has no slot
/// either, so the absence of a slot cannot tell a fresh tree from a wrapped
/// one. The recorded wrapper can.
bool already_wrapped(const UWidget* root) {
    const auto index = root->Index;
    return std::any_of(map_wrappers.begin(), map_wrappers.end(), [index](const auto& entry) {
        return entry.second.overlay == index && wrapped_backdrop(entry.second);
    });
}

/// @brief Wraps the world map's widget-tree root in an overlay carrying a black
///        backdrop, and clips the map to its own box.
///
/// The map content draws past its arranged box, so on a screen wider than the
/// box the map itself would fill the sides. Clipping it and painting black
/// behind it leaves the sides black instead. Both widgets live inside the map's
/// own tree, which ties their lifetime and their parent's visibility to the
/// map rather than to a viewport-level overlay of our own.
///
/// @param widget The UUserWidget being rebuilt.
/// @return A log line when this call wrapped the map, or the failure that
///         stopped it, and nothing when there was nothing to do.
std::string wrap_map_layout(UWidget* widget) {
    static const auto map_class = UKismetStringLibrary::Conv_StringToName(L"WBP_WldMapLayout_C");
    if (widget->Class->Name != map_class || !widget->IsA(UUserWidget::StaticClass())) {
        return {};
    }
    auto tree = static_cast<UUserWidget*>(widget)->WidgetTree;
    auto root = live(tree) ? tree->RootWidget : nullptr;
    // Only wrap a widget-tree root, never reparent a content child.
    if (!live(root) || root->Slot || already_wrapped(root)) {
        return {};
    }

    WrapGuard guard(wrapping_map);

    auto overlay =
        static_cast<UOverlay*>(UGameplayStatics::SpawnObject(UOverlay::StaticClass(), tree));
    auto black = spawn_backdrop_image(tree);
    if (!overlay || !black) {
        return "Map backdrop: allocation failed.\n";
    }
    if (!fill_overlay(overlay, black) || !fill_overlay(overlay, root)) {
        return "Map backdrop: slot failed.\n";
    }
    root->SetClipping(EWidgetClipping::ClipToBoundsAlways);
    overlay->SetVisibility(mirrored_visibility(root->Visibility));
    tree->RootWidget = overlay;
    map_wrappers[root->GetFullName()] = {overlay->Index, black->Index, black->GetFullName()};
    return "Map backdrop: wrapped and clipped " + widget->GetFullName() + "\n";
}

/// @brief Carries a wrapped map's visibility onto the overlay holding its
///        backdrop, so the backdrop is dismissed with the map.
///
/// @param widget The widget whose visibility the game is setting.
/// @param visibility The new ESlateVisibility.
void mirror_map_visibility(UWidget* widget, std::int32_t visibility) {
    const auto found = map_wrappers.find(widget->GetFullName());
    auto parent = live(widget->Slot) ? widget->Slot->Parent : nullptr;
    if (found == map_wrappers.end() || !live(parent) || found->second.overlay != parent->Index) {
        return;
    }
    const auto value = mirrored_visibility(static_cast<ESlateVisibility>(visibility));
    if (parent->Visibility != value) {
        parent->SetVisibility(value);
    }
}

/// @brief Re-expands every backdrop this fix has wrapped.
///
/// The backdrop fills the map's own box, so it is stretched out to the viewport
/// the same way a movie's is. The expansion is absolute, so running it from
/// either callback converges rather than accumulating.
///
/// @return The log lines of whichever backdrops changed.
std::string refresh_map_backdrops() {
    std::string report;
    for (const auto& [content_name, wrapper] : map_wrappers) {
        auto black = wrapped_backdrop(wrapper);
        if (!black) {
            continue;
        }
        report += expand_backdrop(black, "map_backdrop:" + wrapper.black_name,
                                  "Map backdrop: " + content_name);
    }
    return report;
}

} // namespace

/// @brief Keeps the world map's black backdrop wrapped, dismissed and stretched.
///
/// Driven by the two shared widget callbacks rather than a timer: the rebuild
/// that runs before a tree root is consumed, which is the one moment the root
/// can be replaced, and every visibility change, which is what dismisses the
/// wrapper with the map.
///
/// @param address The widget, from RBX at the rebuild or RCX at SetVisibility.
/// @param visibility The new ESlateVisibility, or -1 when called for a rebuild.
/// @return A log line for whichever step changed something.
rust::String update_map_backdrop(std::size_t address, std::int32_t visibility) {
    if (wrapping_map) {
        return {};
    }
    auto widget = reinterpret_cast<UWidget*>(address);
    if (!live(widget)) {
        return {};
    }
    std::string report;
    if (visibility < 0) {
        report = wrap_map_layout(widget);
    } else {
        mirror_map_visibility(widget, visibility);
    }
    return report + refresh_map_backdrops();
}
} // namespace ffrs
