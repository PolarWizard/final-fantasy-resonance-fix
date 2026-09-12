#include "utils.hpp"

namespace ffrs {
namespace {

/// @brief A movie root this mod has wrapped, identified by object index.
///
/// Stores identities only; the live parent is derived from the current widget,
/// so a stale entry is detected rather than dereferenced.
struct Wrapper {
    int overlay;            ///< The overlay that replaced the tree root.
    int black;              ///< The black image behind the movie.
    std::string black_name; ///< Full name of @ref black, to confirm the index still refers to it.
};

std::unordered_map<std::string, Wrapper> wrapped_roots;

/// Set while wrap_movie_root spawns widgets, so the visibility changes that
/// spawning triggers are not mistaken for the game dismissing a movie.
thread_local bool wrapping = false;

/// @brief Re-expands every backdrop this mod has wrapped behind a movie.
///
/// The wrapper fills the movie widget, so when an ancestor is held inside 16:9
/// the backdrop is arranged 16:9 as well and the letterbox columns outside it
/// would show the world instead of black.
///
/// @return The concatenated log lines of whichever backdrops changed.
std::string expand_movie_backdrops() {
    std::string report;
    for (const auto& [root_name, wrapper] : wrapped_roots) {
        auto object = wrapper.black < UObject::GObjects->Num()
                          ? UObject::GObjects->GetByIndex(wrapper.black)
                          : nullptr;
        if (!live(object) || object->GetFullName() != wrapper.black_name) {
            continue;
        }
        report += expand_backdrop(static_cast<UImage*>(object), "backdrop:" + root_name,
                                  "Movie backdrop: " + root_name);
    }
    return report;
}

/// @brief The visibility a wrapper takes from the widget it wraps.
///
/// Visible becomes SelfHitTestInvisible so the overlay never takes input the
/// movie inside it would not have taken.
ESlateVisibility wrapper_visibility(ESlateVisibility value) {
    return value == ESlateVisibility::Visible ? ESlateVisibility::SelfHitTestInvisible : value;
}

/// @brief The widget-tree root to wrap, if this user widget has one.
/// @return The root, or nullptr when this is not a movie or cannot be wrapped.
UWidget* wrappable_movie_root(UUserWidget* user) {
    auto tree = user->WidgetTree;
    if (!tree || !tree->RootWidget) {
        return nullptr;
    }
    auto root = tree->RootWidget;
    const auto name = root->GetName();
    if (name != "Image_Movie" && name != "Projector") {
        return nullptr;
    }
    // Only wrap a widget-tree root, never reparent a content child.
    return root->Slot ? nullptr : root;
}

/// @brief Spawns the opaque black image that sits behind a letterboxed movie.
/// @return The image, or nullptr if the allocation failed.
UImage* spawn_backdrop(UWidgetTree* tree) {
    auto black = static_cast<UImage*>(UGameplayStatics::SpawnObject(UImage::StaticClass(), tree));
    if (!black) {
        return nullptr;
    }
    auto brush = black->Brush;
    brush.DrawAs = ESlateBrushDrawType::Image;
    black->SetBrush(brush);
    black->SetColorAndOpacity(FLinearColor{0, 0, 0, 1});
    black->SetVisibility(ESlateVisibility::HitTestInvisible);
    return black;
}

/// @brief Adds a child that fills the overlay on both axes.
/// @return Whether the overlay gave the child a slot.
bool add_filling(UOverlay* overlay, UWidget* child) {
    auto slot = overlay->AddChildToOverlay(child);
    if (!slot) {
        return false;
    }
    slot->SetHorizontalAlignment(EHorizontalAlignment::HAlign_Fill);
    slot->SetVerticalAlignment(EVerticalAlignment::VAlign_Fill);
    return true;
}

} // namespace

/// @brief Letterboxes a summon's movie image to 16:9.
///
/// The image is fitted to its own arranged box, not to the viewport: an
/// ancestor held inside 16:9 already hands it a 16:9 box, and scaling that by
/// the viewport's wider aspect would letterbox a second time.
///
/// @param address The widget, from RCX at native UpdateRenderTransform.
/// @return A log line when the scale changed, plus any backdrop lines.
rust::String on_movie_transform(std::size_t address) {
    auto widget = reinterpret_cast<UWidget*>(address);
    static const auto movie_name = UKismetStringLibrary::Conv_StringToName(L"Image_Movie");
    if (widget->Name != movie_name) {
        return {};
    }
    const auto box = layout_box(widget);
    if (!arranged(box.size)) {
        return {};
    }
    const auto fit = fit_16_9(box.size.X, box.size.Y);
    const FVector2D next{fit.x, fit.y};
    const bool refit = !approximately(widget->RenderTransform.Scale.X, next.X)
                       || !approximately(widget->RenderTransform.Scale.Y, next.Y);
    // Native UpdateRenderTransform consumes this field immediately after the
    // hook, so it is written every time even when the value is unchanged.
    widget->RenderTransform.Scale = next;
    if (!refit) {
        return expand_movie_backdrops();
    }
    return hud_report("summon_movie", "Native summon image: " + describe(box, fit))
           + expand_movie_backdrops();
}

/// @brief Letterboxes a playing FMV to 16:9.
///
/// Invoked by the native movie frame and texture-assignment hooks, never by a
/// timer, so the fit is re-applied exactly when the engine touches the image.
///
/// @param object A UCPP_WBP_MovieScreen when @p is_screen, else a UCPP_WBP_MovieProjector.
/// @param is_screen Which of the two shapes @p object has.
/// @return A log line when the scale changed, plus any backdrop lines.
rust::String on_native_movie(std::size_t object, bool is_screen) {
    auto projector = is_screen ? reinterpret_cast<UCPP_WBP_MovieScreen*>(object)->Projector
                               : reinterpret_cast<UCPP_WBP_MovieProjector*>(object);
    if (!projector || !projector->Projector) {
        return {};
    }
    auto picture = projector->Projector;
    const auto box = layout_box(picture);
    if (!arranged(box.size)) {
        return {};
    }
    const auto fit = fit_16_9(box.size.X, box.size.Y);
    // GetFullName walks the outer chain building a string, so the key is only
    // built on a frame that actually refits.
    if (!apply_center_scale(picture, fit)) {
        return expand_movie_backdrops();
    }
    const std::string label = is_screen ? "OnFrame" : "SetManaTexture";
    return hud_report("native_movie:" + projector->GetFullName(),
                      std::format("Native FMV: {}, {}", label, describe(box, fit)))
           + expand_movie_backdrops();
}

/// @brief Carries a wrapped movie's visibility onto the wrapper holding its backdrop.
///
/// Without this the black backdrop would outlive the movie it sits behind. A
/// Visible movie becomes SelfHitTestInvisible on the wrapper so the overlay
/// never takes input the movie itself would not have taken.
///
/// @param address The widget, from RCX at native UWidget::SetVisibility.
/// @param visibility The new ESlateVisibility, from DL.
void sync_movie_backdrop(std::size_t address, std::uint8_t visibility) {
    if (wrapping) {
        return;
    }
    auto widget = reinterpret_cast<UWidget*>(address);
    if (!widget->Slot) {
        return;
    }
    auto parent = widget->Slot->Parent;
    if (!parent) {
        return;
    }
    const auto found = wrapped_roots.find(widget->GetFullName());
    if (found == wrapped_roots.end() || found->second.overlay != parent->Index) {
        return;
    }
    const auto value = wrapper_visibility(static_cast<ESlateVisibility>(visibility));
    if (parent->Visibility != value) {
        parent->SetVisibility(value);
    }
}

/// @brief Wraps a movie's widget-tree root in an overlay carrying a black backdrop.
///
/// Runs from the user-widget rebuild, before the tree root is consumed for the
/// Slate rebuild, which is the one moment the root can be replaced. The overlay
/// holds the black image behind the movie and the movie above it, both filling
/// the overlay, so the bars beside a letterboxed movie are black rather than
/// the world behind it.
///
/// @param address The UUserWidget, from RBX at the rebuild.
/// @return A log line naming what was wrapped, or the failure that stopped it.
rust::String wrap_movie_root(std::size_t address) {
    if (wrapping) {
        return {};
    }
    auto user = reinterpret_cast<UUserWidget*>(address);
    auto root = wrappable_movie_root(user);
    if (!root) {
        return {};
    }

    struct Scope {
        Scope() {
            wrapping = true;
        }

        ~Scope() {
            wrapping = false;
        }
    } scope;

    auto tree = user->WidgetTree;
    auto overlay =
        static_cast<UOverlay*>(UGameplayStatics::SpawnObject(UOverlay::StaticClass(), tree));
    auto black = spawn_backdrop(tree);
    if (!overlay || !black) {
        return "Movie backdrop: allocation failed.";
    }
    if (!add_filling(overlay, black)) {
        return "Movie backdrop: background slot failed.";
    }
    if (!add_filling(overlay, root)) {
        return "Movie backdrop: movie slot failed.";
    }

    overlay->SetVisibility(wrapper_visibility(root->Visibility));
    tree->RootWidget = overlay;
    wrapped_roots[root->GetFullName()] = {overlay->Index, black->Index, black->GetFullName()};
    return std::string("Native movie backdrop: wrapped ") + user->GetName() + "." + root->GetName();
}
} // namespace ffrs
