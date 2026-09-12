#include "bridge.hpp"
#include "final-fantasy-resonance/src/bridge.rs.h"
#include <algorithm>
#include <cmath>

/// @file
/// The layout arithmetic every correction is built on. Pure functions of their
/// arguments with no engine access, which is what lets src/tests/layout.rs
/// exercise them from Rust.
///
/// Each returns an identity result for geometry it cannot use. Callers do check
/// their geometry first, so that is a contract rather than a live path: these
/// feed render transforms, and the failure is silent -- an aspect of 0 reaching
/// letterbox_inset would inset the full width and collapse the HUD to nothing.

namespace ffrs {

/// @brief Letterboxes a box to 16:9 about its center.
///
/// The axis already at or inside 16:9 keeps scale 1, so a movie is only ever
/// shrunk within the box Slate arranged for it and never spills past it: a 32:9
/// box scales 0.5,1 while a box an ancestor already holds inside 16:9 scales
/// 1,1 and is left alone.
///
/// @return The scale for each axis, or 1,1 for unusable geometry.
Scale fit_16_9(double width, double height) noexcept {
    if (!std::isfinite(width) || !std::isfinite(height) || width <= 0 || height <= 0) {
        return {1, 1};
    }
    const double ratio = (16.0 / 9.0) * height / width;
    return {std::min(1.0, ratio), std::min(1.0, 1.0 / ratio)};
}

/// @brief The render scale that paints a widget across the whole viewport.
///
/// An axis is only stretched when the box already spans the viewport on the
/// other one, which is what separates a full-bleed backdrop missing its side
/// columns from a panel the game sized deliberately and must keep. A box that
/// already fills the viewport scales 1,1.
///
/// @note Both sizes must be in the same units -- local (UMG) ones -- since a
///       box comes from cached geometry while the viewport reports physical
///       pixels. Mismatched units find no spanned axis and expand nothing.
/// @return The scale for each axis, or 1,1 for unusable geometry.
Scale full_bleed_scale(double box_width, double box_height, double screen_width,
                       double screen_height) noexcept {
    for (double value : {box_width, box_height, screen_width, screen_height}) {
        if (!std::isfinite(value) || value <= 0) {
            return {1, 1};
        }
    }
    const auto spans = [](double a, double b) { return std::abs(a - b) <= 1.0; };
    return {spans(box_height, screen_height) ? screen_width / box_width : 1.0,
            spans(box_width, screen_width) ? screen_height / box_height : 1.0};
}

/// @brief Applies an inset to a value the game also writes, without accumulating.
///
/// Any @p current the state did not write itself becomes the new baseline, so
/// the game's authored value survives and the result is baseline+inset however
/// often the transform updates. A changed inset re-derives from the same
/// baseline rather than stacking on the previous result.
///
/// @param state Per-widget, per-axis baseline; must outlive the correction.
/// @param current The value on the widget right now.
/// @param inset The amount to apply to the authored value.
/// @return The value to write back.
float apply_offset(OffsetState& state, float current, float inset) noexcept {
    if (!state.initialized || std::abs(current - state.last_written) > 0.00001f) {
        state.baseline = current;
        state.initialized = true;
    }
    state.last_written = state.baseline + inset;
    return state.last_written;
}

/// @brief The inset from each edge of a box holding a centered @p aspect rectangle.
///
/// The axis already at or inside @p aspect keeps its full extent and insets 0,
/// so the rectangle is only ever held within the box: a 3840x1080 box at 16:9
/// insets 960,0, and the same box at 32:9 insets nothing.
///
/// @note The result divided by the box is a ratio of the box, so an anchor
///       derived from it is identical at any resolution or DPI.
/// @return The inset on each axis, or 0,0 for unusable geometry.
Inset letterbox_inset(double width, double height, double aspect) noexcept {
    if (!std::isfinite(width) || !std::isfinite(height) || !std::isfinite(aspect) || width <= 0
        || height <= 0 || aspect <= 0) {
        return {};
    }
    return {(width - std::min(width, height * aspect)) * 0.5,
            (height - std::min(height, width / aspect)) * 0.5};
}
} // namespace ffrs
