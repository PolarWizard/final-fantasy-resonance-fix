#[cxx::bridge(namespace = "ffrs")]
pub mod ffi {
    pub struct OffsetState {
        pub baseline: f32,
        pub last_written: f32,
        pub initialized: bool,
    }
    pub struct Scale {
        pub x: f64,
        pub y: f64,
    }
    pub struct Inset {
        pub x: f64,
        pub y: f64,
    }
    unsafe extern "C++" {
        include!("cpp/bridge.hpp");
        // Production calls this helper from C++; Rust exercises it in tests.
        #[allow(dead_code)]
        fn letterbox_inset(width: f64, height: f64, aspect: f64) -> Inset;
        // Production calls this helper from C++; Rust exercises it in tests.
        #[allow(dead_code)]
        fn fit_16_9(width: f64, height: f64) -> Scale;
        // Production calls this helper from C++; Rust exercises it in tests.
        #[allow(dead_code)]
        fn full_bleed_scale(
            box_width: f64,
            box_height: f64,
            screen_width: f64,
            screen_height: f64,
        ) -> Scale;
        #[allow(dead_code)]
        fn apply_offset(state: &mut OffsetState, current: f32, inset: f32) -> f32;
        unsafe fn constrain_hud_slot(widget: usize, slot: usize, path: u32, aspect: f64) -> String;
        unsafe fn correct_tooltip_position(widget: usize, aspect: f64) -> String;
        unsafe fn expand_menu_backdrop(widget: usize) -> String;
        unsafe fn wrap_movie_root(widget: usize) -> String;
        unsafe fn sync_movie_backdrop(widget: usize, visibility: u8);
        unsafe fn on_movie_transform(widget: usize) -> String;
        unsafe fn on_native_movie(object: usize, is_screen: bool) -> String;
    }
}

#[cfg(test)]
#[path = "tests/layout.rs"]
mod tests;
