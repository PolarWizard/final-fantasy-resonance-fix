#include "bridge.hpp"
#include "final-fantasy-resonance-fix/src/bridge.rs.h"
#include "Engine_structs.hpp"

/// @file
/// @brief Applies TOML effect switches to the final per-view SDK settings.
///
/// The shared signature hook calls this code after native post-processing
/// adjustments. Writes affect the current view; later views receive the same
/// overrides when they reach the hook. Custom materials, HUD artwork, and
/// effects baked into movie frames are outside these controls.

namespace ffrs {

/// @brief Suppresses selected effects in a view's final post-process settings.
///
/// @details Disabled effects have their amount or intensity cleared. Depth of
/// field also clears focal distance, scale, and depth-blur radius: nonpositive
/// focal distance selects the native diaphragm calculation's zero-blur branch,
/// while scale and radius cover the other depth-of-field contributions.
/// Aperture and depth-blur amount remain unchanged, including the amount used
/// as a divisor by the native depth-blur calculation.
///
/// Applying the same options repeatedly is idempotent. Setting an option back
/// to true leaves the supplied value alone; it does not restore a value cleared
/// by an earlier call. Normal values come from the engine's next blended view.
///
/// @param settings Address of the writable SDK::FPostProcessSettings in the view.
/// @param options Effect switches loaded from the TOML configuration.
/// @pre settings points to a valid, aligned settings object for the duration of
///      this call, after the engine has finished blending its settings.
/// @pre The caller owns access to the view; this function does not synchronize
///      concurrent reads or writes to the settings.
/// @note Preserved effects and blending override bits are left untouched.
void apply_post_processing(std::size_t settings, const PostProcessOptions& options) noexcept {
    auto& post = *reinterpret_cast<SDK::FPostProcessSettings*>(settings);
    // These are already-blended view settings. Override bits belong to the
    // earlier blending stage; changing them here would serve no purpose.
    if (!options.motion_blur) {
        post.MotionBlurAmount = 0.0f;
    }
    if (!options.depth_of_field) {
        // The native diaphragm CoC calculation takes its zero-blur branch
        // when focal distance <= 0. Scale also covers the other DOF path;
        // radius suppresses the separate distance-based depth blur.
        post.DepthOfFieldFocalDistance = 0.0f;
        post.DepthOfFieldScale = 0.0f;
        post.DepthOfFieldDepthBlurRadius = 0.0f;
    }
    if (!options.bloom) {
        post.BloomIntensity = 0.0f;
    }
    if (!options.film_grain) {
        post.FilmGrainIntensity = 0.0f;
    }
    if (!options.chromatic_aberration) {
        post.SceneFringeIntensity = 0.0f;
    }
    if (!options.vignette) {
        post.VignetteIntensity = 0.0f;
    }
}

} // namespace ffrs
