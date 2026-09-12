use super::ffi::{OffsetState, apply_offset, fit_16_9, full_bleed_scale, letterbox_inset};

#[test]
fn offset_preserves_authored_value_without_accumulating_on_ticks_or_resize() {
    let mut state = super::ffi::OffsetState {
        baseline: 0.0,
        last_written: 0.0,
        initialized: false,
    };
    let mut padding = super::ffi::apply_offset(&mut state, 12.0, 960.0);
    assert_eq!(padding, 972.0);
    for _ in 0..100 {
        padding = super::ffi::apply_offset(&mut state, padding, 960.0);
        assert_eq!(padding, 972.0);
    }
    padding = super::ffi::apply_offset(&mut state, padding, 440.0);
    assert_eq!(padding, 452.0);
    padding = super::ffi::apply_offset(&mut state, padding, 0.0);
    assert_eq!(padding, 12.0);
}
#[test]
fn offset_respects_game_reset() {
    let mut state = super::ffi::OffsetState {
        baseline: 0.0,
        last_written: 0.0,
        initialized: false,
    };
    assert_eq!(super::ffi::apply_offset(&mut state, 0.0, 960.0), 960.0);
    assert_eq!(super::ffi::apply_offset(&mut state, 20.0, 960.0), 980.0);
    assert_eq!(super::ffi::apply_offset(&mut state, 980.0, 0.0), 20.0);
}
#[test]
fn each_constrain_preset_insets_the_root_by_its_own_ratio() {
    // A 3840x1080 viewport in local units. A wider preset leaves a narrower
    // letterbox column each side, and the preset matching the screen leaves
    // none. The height is spanned either way, so it never insets.
    for (aspect, expected) in [(16.0 / 9.0, 960.0), (21.0 / 9.0, 660.0), (32.0 / 9.0, 0.0)] {
        let inset = letterbox_inset(3840.0, 1080.0, aspect);
        assert!(
            (inset.x - expected).abs() < 1e-9,
            "aspect {aspect}: inset {} is not {expected}",
            inset.x
        );
        assert_eq!(inset.y, 0.0);
    }
    // A box taller than the ratio insets on the other axis instead.
    let narrow = letterbox_inset(1280.0, 1024.0, 16.0 / 9.0);
    assert_eq!((narrow.x, narrow.y), (0.0, 152.0));
    // A box already at the ratio is held exactly as it is.
    let native = letterbox_inset(1920.0, 1080.0, 16.0 / 9.0);
    assert_eq!((native.x, native.y), (0.0, 0.0));
}

#[test]
fn the_root_rectangle_rejects_invalid_geometry() {
    for (w, h, aspect) in [
        (0.0, 1080.0, 16.0 / 9.0),
        (1920.0, 0.0, 16.0 / 9.0),
        (f64::NAN, 1080.0, 16.0 / 9.0),
        (f64::INFINITY, 1080.0, 16.0 / 9.0),
        (3840.0, 1080.0, 0.0),
        (3840.0, 1080.0, -1.0),
        (3840.0, 1080.0, f64::NAN),
    ] {
        let inset = letterbox_inset(w, h, aspect);
        assert_eq!((inset.x, inset.y), (0.0, 0.0));
    }
}

#[test]
fn tooltip_offset_preserves_authored_translation_on_ticks_resize_and_reset() {
    let mut state = OffsetState {
        baseline: 0.0,
        last_written: 0.0,
        initialized: false,
    };
    let first = apply_offset(&mut state, 12.0, -960.0);
    assert_eq!(first, -948.0);
    assert_eq!(apply_offset(&mut state, first, -960.0), first);
    let resized = apply_offset(&mut state, first, -480.0);
    assert_eq!(resized, -468.0);
    assert_eq!(apply_offset(&mut state, resized, 0.0), 12.0);
    assert_eq!(apply_offset(&mut state, 20.0, -960.0), -940.0);
}

#[test]
fn movie_fit_measures_the_box_it_is_given_not_the_screen() {
    // A movie filling an ultrawide viewport is letterboxed horizontally.
    let wide = fit_16_9(7680.0, 2160.0);
    assert_eq!((wide.x, wide.y), (0.5, 1.0));
    assert_eq!(fit_16_9(3840.0, 1080.0).x, 0.5);
    // The same movie inside an ancestor already held at 16:9 is already
    // correct, so it must be left alone rather than letterboxed a second time.
    for (w, h) in [(3840.0, 2160.0), (1920.0, 1080.0), (1280.0, 720.0)] {
        let fit = fit_16_9(w, h);
        assert_eq!((fit.x, fit.y), (1.0, 1.0));
    }
    // A box taller than 16:9 letterboxes on the other axis instead.
    let tall = fit_16_9(1280.0, 1024.0);
    assert_eq!(tall.x, 1.0);
    assert!((tall.y - 0.703125).abs() < 1e-9);
    // Either way the visible movie ends up 16:9 within the box.
    for (w, h) in [(7680.0, 2160.0), (3440.0, 1440.0), (1280.0, 1024.0)] {
        let fit = fit_16_9(w, h);
        assert!(((w * fit.x) / (h * fit.y) - 16.0 / 9.0).abs() < 1e-9);
    }
}

#[test]
fn movie_fit_leaves_invalid_geometry_unscaled() {
    for (w, h) in [
        (0.0, 1080.0),
        (1920.0, 0.0),
        (-1920.0, 1080.0),
        (f64::NAN, 1080.0),
        (f64::INFINITY, 1080.0),
    ] {
        let fit = fit_16_9(w, h);
        assert_eq!((fit.x, fit.y), (1.0, 1.0));
    }
}

#[test]
fn full_bleed_backdrop_stretches_out_of_a_16_by_9_root() {
    // A 32:9 viewport is 3840x1080 local units at viewport scale 2, and a root
    // held inside 16:9 arranges to 1920x1080: the backdrop spans the height, so
    // it stretches horizontally and keeps its height. Menu dimming logged
    // exactly this 2,1 while the root was positioned by setters.
    let inside = full_bleed_scale(1920.0, 1080.0, 3840.0, 1080.0);
    assert_eq!((inside.x, inside.y), (2.0, 1.0));
    // A box that already fills the viewport is left alone.
    let filling = full_bleed_scale(3840.0, 1080.0, 3840.0, 1080.0);
    assert_eq!((filling.x, filling.y), (1.0, 1.0));
    // A root inset vertically instead stretches on the other axis.
    let tall = full_bleed_scale(1280.0, 720.0, 1280.0, 1024.0);
    assert_eq!((tall.x, tall.y), (1.0, 1024.0 / 720.0));
    // A panel short of the viewport on both axes was sized deliberately.
    let panel = full_bleed_scale(800.0, 450.0, 3840.0, 1080.0);
    assert_eq!((panel.x, panel.y), (1.0, 1.0));
}

#[test]
fn full_bleed_scale_needs_both_sizes_in_the_same_units() {
    // A local-unit box compared against physical pixels finds no spanned axis
    // and expands nothing, which is what left movie bars unstretched.
    let mismatched = full_bleed_scale(1920.0, 1080.0, 7680.0, 2160.0);
    assert_eq!((mismatched.x, mismatched.y), (1.0, 1.0));
    for (bw, bh, sw, sh) in [
        (0.0, 1080.0, 3840.0, 1080.0),
        (1920.0, 1080.0, 3840.0, 0.0),
        (-1920.0, 1080.0, 3840.0, 1080.0),
        (f64::NAN, 1080.0, 3840.0, 1080.0),
        (1920.0, 1080.0, f64::INFINITY, 1080.0),
    ] {
        let fit = full_bleed_scale(bw, bh, sw, sh);
        assert_eq!((fit.x, fit.y), (1.0, 1.0));
    }
}
