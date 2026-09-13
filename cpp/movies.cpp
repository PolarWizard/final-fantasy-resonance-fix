#include "utils.hpp"

namespace ffrs {
namespace {

/// Every movie root this mod has wrapped, by the root's full name.
std::unordered_map<std::string, BackdropWrapper> wrapped_roots;

/// Set while wrap_movie_root spawns widgets, so the visibility changes that
/// spawning triggers are not mistaken for the game dismissing a movie.
thread_local bool wrapping = false;

/// Whether a sequence has put a movie on screen, as the sequence itself says.
///
/// A summon authors UISetVisibility with its flag clear when its cinematic
/// takes the whole screen, and a sequence that names its movie directly
/// authors UISetVisibilityMovie. The later event wins, which is the sequence's
/// latest instruction either way.
bool movie_phase = false;

/// @brief Whether a wrapped root is a cinematic's movie image.
///
/// A cinematic's movie image shares its layout with an in-engine lead-in, so
/// its backdrop waits for the sequence. A movie screen or movie widget shows
/// nothing but a movie, and keeps following its own visibility.
bool waits_for_sequence(const UWidget* root) {
    static const auto movie_name = UKismetStringLibrary::Conv_StringToName(L"Image_Movie");
    return root && root->Name == movie_name;
}

/// @brief Whether the black image behind a movie should be drawn yet.
///
/// The gate belongs on the backdrop alone. The wrapper it hangs in is the
/// cinematic's widget-tree root and holds the movie image as its other child,
/// so hiding the wrapper takes the movie down with the backdrop and the
/// cinematic shows nothing at all. Hidden rather than Collapsed keeps the
/// image arranged, so the geometry the expansion measures stays available and
/// the bars are already the right size when they are drawn.
ESlateVisibility backdrop_visibility(const UWidget* root) {
    return waits_for_sequence(root) && !movie_phase ? ESlateVisibility::Hidden
                                                    : ESlateVisibility::HitTestInvisible;
}

/// @brief Re-gates and re-expands every backdrop this mod has wrapped.
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
/// registry keeps holding identities only.
///
/// @return The concatenated log lines of whichever wrappers changed.
std::string refresh_movie_backdrops() {
    std::string report;
    for (const auto& [root_name, wrapper] : wrapped_roots) {
        auto black = live_backdrop(wrapper);
        if (!black) {
            continue;
        }
        auto overlay = live(black->Slot) ? black->Slot->Parent : nullptr;
        auto root =
            live(overlay) && overlay->GetChildrenCount() > 1 ? overlay->GetChildAt(1) : nullptr;
        if (live(root)) {
            const auto value = backdrop_visibility(root);
            if (black->Visibility != value) {
                black->SetVisibility(value);
                report += hud_report("gate:" + root_name,
                                     std::format("Movie backdrop gate: {}, drawn={}", root_name,
                                                 value == ESlateVisibility::HitTestInvisible));
            }
        }
        report += expand_backdrop(black, "backdrop:" + root_name, "Movie backdrop: " + root_name);
    }
    return report;
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

} // namespace

/// @brief Carries a battle sequence's own decisions to the movie backdrop.
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
/// @param parameter The FBTL_SEQUENCE_PARAMETER, from R8 where the dispatch
///                  reads its EventType.
/// @return A log line for the event, plus any backdrop lines.
rust::String on_battle_sequence_event(std::size_t parameter) {
    auto event = reinterpret_cast<const FBTL_SEQUENCE_PARAMETER*>(parameter);
    if (!event) {
        return {};
    }
    const char* named = nullptr;
    switch (event->EventType) {
    case EeBtlSequenceEventType::UISetVisibilityMovie:
        movie_phase = event->UI_SetVisibillityMovie_IsVisibility;
        named = "movie visibility";
        break;
    case EeBtlSequenceEventType::UISetVisibility:
        movie_phase = !event->UI_SetVisibillity_IsVisibility;
        named = "battle UI hidden";
        break;
    default:
        return {};
    }
    auto report = hud_report(
        "movie_phase", std::format("Battle sequence: {} {}", named, movie_phase ? "on" : "off"));
    // The event names no widget, so every wrapper is asked again.
    return report + refresh_movie_backdrops();
}

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
        return refresh_movie_backdrops();
    }
    return hud_report("summon_movie", "Native summon image: " + describe(box, fit))
           + refresh_movie_backdrops();
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
        return refresh_movie_backdrops();
    }
    const std::string label = is_screen ? "OnFrame" : "SetManaTexture";
    return hud_report("native_movie:" + projector->GetFullName(),
                      std::format("Native FMV: {}, {}", label, describe(box, fit)))
           + refresh_movie_backdrops();
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
    const auto value = mirrored_visibility(static_cast<ESlateVisibility>(visibility));
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

    WrapGuard guard(wrapping);

    auto tree = user->WidgetTree;
    auto overlay =
        static_cast<UOverlay*>(UGameplayStatics::SpawnObject(UOverlay::StaticClass(), tree));
    auto black = spawn_backdrop_image(tree);
    if (!overlay || !black) {
        return "Movie backdrop: allocation failed.";
    }
    if (!fill_overlay(overlay, black)) {
        return "Movie backdrop: background slot failed.";
    }
    if (!fill_overlay(overlay, root)) {
        return "Movie backdrop: movie slot failed.";
    }

    if (waits_for_sequence(root)) {
        // A new cinematic begins with its lead-in, whatever the last one said.
        movie_phase = false;
    }
    black->SetVisibility(backdrop_visibility(root));
    overlay->SetVisibility(mirrored_visibility(root->Visibility));
    tree->RootWidget = overlay;
    wrapped_roots[root->GetFullName()] = {overlay->Index, black->Index, black->GetFullName()};
    return std::string("Native movie backdrop: wrapped ") + user->GetName() + "." + root->GetName();
}
} // namespace ffrs
