#include "utils.hpp"

namespace ffrs {
namespace {

/// The target the back buffer capture was last handed, a context that resolves
/// the world for the draws below, the scratch target those draws bounce
/// through, and whether a capture is waiting to be re-framed. Identities are
/// re-checked before use rather than trusted across frames.
UTextureRenderTarget2D* capture_target = nullptr;
UObject* capture_context = nullptr;
UTextureRenderTarget2D* scratch = nullptr;
std::string scratch_name;
bool pending = false;

/// @brief Draws a region of @p source across the whole of @p destination.
///
/// A canvas is the one route from here to a render target's pixels, and it
/// takes the source rectangle in UV and the destination rectangle in pixels, so
/// a crop and a stretch are a single draw.
///
/// @param destination The render target drawn into, filled edge to edge.
/// @param source The texture sampled.
/// @param origin Where in @p source sampling starts, in UV.
/// @param extent How much of @p source is sampled, in UV.
/// @return Whether the canvas was handed out and the draw issued.
bool draw_region(UTextureRenderTarget2D* destination, UTexture* source, const FVector2D& origin,
                 const FVector2D& extent) {
    UCanvas* canvas = nullptr;
    FVector2D size{};
    FDrawToRenderTargetContext draw{};
    UKismetRenderingLibrary::BeginDrawCanvasToRenderTarget(capture_context, destination, &canvas,
                                                           &size, &draw);
    if (!live(canvas)) {
        return false;
    }
    canvas->K2_DrawTexture(source, FVector2D{}, size, origin, extent,
                           FLinearColor{1.0f, 1.0f, 1.0f, 1.0f}, EBlendMode::BLEND_Opaque, 0.0f,
                           FVector2D{});
    // The destination stays bound to the canvas until this call.
    UKismetRenderingLibrary::EndDrawCanvasToRenderTarget(capture_context, draw);
    return true;
}

/// @brief The scratch target the re-frame reads from, created on demand.
///
/// A draw cannot sample the target it writes, so the captured frame is copied
/// aside and the crop is taken from the copy. Nothing in the game holds this
/// object, so collection may take it between transitions; its name is what
/// notices, and a fresh one is made when it does.
///
/// @return A target in the capture's own size and format, or null.
UTextureRenderTarget2D* bounce_target() {
    if (!live(scratch) || scratch->GetFullName() != scratch_name) {
        scratch = UKismetRenderingLibrary::CreateRenderTarget2D(
            capture_context, capture_target->SizeX, capture_target->SizeY,
            capture_target->RenderTargetFormat, capture_target->ClearColor, false, false);
        scratch_name = live(scratch) ? scratch->GetFullName() : std::string{};
    }
    return live(scratch) ? scratch : nullptr;
}

} // namespace

/// @brief Remembers the render target the screen capture was handed.
///
/// CPP_BackBufferCaptureSubsystem::Capture records a target and raises a
/// request that is honoured afterwards; it does not blit. Taking the pointer
/// here is how the re-frame below knows which target the transition draws,
/// without searching for it, and the request is what tells it a fresh frame is
/// on the way.
///
/// @param context The subsystem, from RCX.
/// @param address The render target, from RDX.
/// @return A log line when the target changes.
rust::String note_capture_target(std::size_t context, std::size_t address) {
    auto subsystem = reinterpret_cast<UObject*>(context);
    auto target = reinterpret_cast<UTextureRenderTarget2D*>(address);
    if (!live(subsystem) || !live(target)) {
        return {};
    }
    capture_context = subsystem;
    capture_target = target;
    pending = true;
    const auto viewport = viewport_local_size(subsystem);
    return hud_report("capture_target",
                      std::format("Capture target: {} is {}x{}, viewport {:g}x{:g}",
                                  target->GetName(), target->SizeX, target->SizeY, viewport.X,
                                  viewport.Y));
}

/// @brief The shape of the frame the encounter transition draws.
///
/// The capture's own target decides it: the copy scales whatever it is given
/// into that target, so the target's proportions are the proportions of the
/// picture the transition presents.
///
/// @return Width over height of the capture target, 16:9 until one is seen.
double captured_aspect() {
    return live(capture_target) && capture_target->SizeY > 0
               ? static_cast<double>(capture_target->SizeX) / capture_target->SizeY
               : 16.0 / 9.0;
}

/// @brief Makes the captured frame a true picture of the capture's own shape.
///
/// The copy behind CPP_BackBufferCaptureSubsystem::Capture scales the whole
/// screen into a target that is 16:9, so on a wider screen the frame arrives
/// horizontally compressed -- an exported capture measured half width. The
/// transition draws that frame as a picture of the target's shape, which is how
/// the compression reaches the screen.
///
/// So the middle of the capture, the part its own shape covers, is stretched
/// back over the whole target: its contents become a picture in its own
/// proportions again, and what the sides lose is width a 16:9 screen never had.
/// Two draws, because a canvas cannot sample the target it writes -- the frame
/// goes out to the scratch whole and comes back cropped.
///
/// Run once per capture, from the transition's own image, which is the first
/// thing to touch the target after the copy the request raised.
///
/// @param address The widget, from RCX at native UpdateRenderTransform.
/// @return A log line naming the crop that was taken.
rust::String reframe_capture(std::size_t address) {
    auto widget = reinterpret_cast<UWidget*>(address);
    static const auto capture_name = UKismetStringLibrary::Conv_StringToName(L"Image_44");
    if (!live(widget) || widget->Name != capture_name || !pending || !live(capture_target)
        || !live(capture_context)) {
        return {};
    }
    const auto viewport = viewport_local_size(capture_context);
    if (!arranged(viewport)) {
        return {};
    }
    // The fraction of the captured width the capture's own shape covers: half
    // of it at 32:9 against a 16:9 capture, and all of it on a screen no wider
    // than the capture, where there is nothing to trim.
    const double keep = std::min(1.0, captured_aspect() * viewport.Y / viewport.X);
    auto bounce = approximately(keep, 1.0) ? nullptr : bounce_target();
    if (!bounce || !draw_region(bounce, capture_target, {}, {1.0, 1.0})) {
        return {};
    }
    pending = false;
    draw_region(capture_target, bounce, {(1.0 - keep) / 2.0, 0.0}, {keep, 1.0});
    return hud_report(
        "reframe_capture",
        std::format("Re-framed capture: kept {:g} of {}x{} at aspect {:g} on {:g}x{:g}", keep,
                    capture_target->SizeX, capture_target->SizeY, captured_aspect(), viewport.X,
                    viewport.Y));
}

} // namespace ffrs
