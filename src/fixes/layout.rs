//! The layout arithmetic every correction is built on: pure functions of their
//! arguments with no engine access.
//!
//! Each returns an identity result for geometry it cannot use. Callers do check
//! their geometry first, so that is a contract rather than a live path: these
//! feed render transforms, and the failure is silent -- an aspect of 0 reaching
//! [`letterbox_inset`] would inset the full width and collapse the HUD to
//! nothing.

/// A render scale, one factor per axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale {
    pub x: f64,
    pub y: f64,
}

/// The inset from each edge of a box, one amount per axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Inset {
    pub x: f64,
    pub y: f64,
}

/// The authored value [`apply_offset`] adds an inset to, and the value it last
/// wrote on top of it.
#[derive(Clone, Copy, Debug, Default)]
pub struct OffsetState {
    pub baseline: f32,
    pub last_written: f32,
    pub initialized: bool,
}

/// Letterboxes a box to 16:9 about its center.
///
/// The axis already at or inside 16:9 keeps scale 1, so a movie is only ever
/// shrunk within the box Slate arranged for it and never spills past it: a 32:9
/// box scales 0.5,1 while a box an ancestor already holds inside 16:9 scales
/// 1,1 and is left alone.
///
/// Returns the scale for each axis, or 1,1 for unusable geometry.
pub fn fit_16_9(width: f64, height: f64) -> Scale {
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Scale { x: 1.0, y: 1.0 };
    }
    let ratio = (16.0 / 9.0) * height / width;
    Scale {
        x: ratio.min(1.0),
        y: (1.0 / ratio).min(1.0),
    }
}

/// The horizontal render scale that carries a full-height widget pinned to one
/// side of a centered box out to the same side of the viewport, its other edge
/// held in place.
///
/// As in [`full_bleed_scale`], the box must span the viewport's height, which
/// is what makes the columns either side of it letterbox. `width` is the
/// widget's own; all sizes are in local units.
///
/// Returns the scale, or 1 for unusable geometry.
pub fn side_bleed_scale(
    width: f64,
    box_width: f64,
    box_height: f64,
    screen_width: f64,
    screen_height: f64,
) -> f64 {
    if [width, box_width, box_height, screen_width, screen_height]
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0)
        || (box_height - screen_height).abs() > 1.0
    {
        return 1.0;
    }
    (width + ((screen_width - box_width) / 2.0).max(0.0)) / width
}

/// The render scale that paints a widget across the whole viewport.
///
/// An axis is only stretched when the box already spans the viewport on the
/// other one, which is what separates a full-bleed backdrop missing its side
/// columns from a panel the game sized deliberately and must keep. A box that
/// already fills the viewport scales 1,1.
///
/// Both sizes must be in the same units -- local (UMG) ones -- since a box
/// comes from cached geometry while the viewport reports physical pixels.
/// Mismatched units find no spanned axis and expand nothing.
///
/// Returns the scale for each axis, or 1,1 for unusable geometry.
pub fn full_bleed_scale(
    box_width: f64,
    box_height: f64,
    screen_width: f64,
    screen_height: f64,
) -> Scale {
    if [box_width, box_height, screen_width, screen_height]
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0)
    {
        return Scale { x: 1.0, y: 1.0 };
    }
    let spans = |a: f64, b: f64| (a - b).abs() <= 1.0;
    Scale {
        x: if spans(box_height, screen_height) {
            screen_width / box_width
        } else {
            1.0
        },
        y: if spans(box_width, screen_width) {
            screen_height / box_height
        } else {
            1.0
        },
    }
}

/// Applies an inset to a value the game also writes, without accumulating.
///
/// Any `current` the state did not write itself becomes the new baseline, so
/// the game's authored value survives and the result is baseline+inset however
/// often the transform updates. A changed inset re-derives from the same
/// baseline rather than stacking on the previous result.
///
/// `state` is per widget and per axis, and must outlive the correction.
/// Returns the value to write back.
pub fn apply_offset(state: &mut OffsetState, current: f32, inset: f32) -> f32 {
    if !state.initialized || (current - state.last_written).abs() > 0.00001 {
        state.baseline = current;
        state.initialized = true;
    }
    state.last_written = state.baseline + inset;
    state.last_written
}

/// The inset from each edge of a box holding a centered `aspect` rectangle.
///
/// The axis already at or inside `aspect` keeps its full extent and insets 0,
/// so the rectangle is only ever held within the box: a 3840x1080 box at 16:9
/// insets 960,0, and the same box at 32:9 insets nothing.
///
/// The result divided by the box is a ratio of the box, so an anchor derived
/// from it is identical at any resolution or DPI. Returns 0,0 for unusable
/// geometry.
pub fn letterbox_inset(width: f64, height: f64, aspect: f64) -> Inset {
    if !width.is_finite()
        || !height.is_finite()
        || !aspect.is_finite()
        || width <= 0.0
        || height <= 0.0
        || aspect <= 0.0
    {
        return Inset { x: 0.0, y: 0.0 };
    }
    Inset {
        x: (width - width.min(height * aspect)) * 0.5,
        y: (height - height.min(width / aspect)) * 0.5,
    }
}
