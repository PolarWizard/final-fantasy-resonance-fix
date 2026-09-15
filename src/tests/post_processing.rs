use super::ffi;
use crate::config::{Config, PostProcessing};

// The dumped FPostProcessSettings is 0x760 bytes. Exercise the real C++ writes
// through the bridge against aligned storage, checking every byte so a bad
// field offset or an unintended neighbor write fails the test.
#[repr(align(16))]
struct Settings([u8; 0x760]);

#[test]
fn effect_switches_only_clear_their_own_fields() {
    for disabled in 0u8..64 {
        let mut options = ffi::PostProcessOptions::from(PostProcessing::default());
        options.motion_blur = disabled & 1 == 0;
        options.depth_of_field = disabled & 2 == 0;
        options.bloom = disabled & 4 == 0;
        options.film_grain = disabled & 8 == 0;
        options.chromatic_aberration = disabled & 16 == 0;
        options.vignette = disabled & 32 == 0;

        let mut actual = Settings([0x3f; 0x760]);
        let mut expected = actual.0;
        for (bit, offsets) in [
            (1, &[0x6d0][..]),
            (2, &[0x658, 0x6b8, 0x660][..]),
            (4, &[0x304][..]),
            (8, &[0x5d0][..]),
            (16, &[0x2fc][..]),
            (32, &[0x5c0][..]),
        ] {
            if disabled & bit != 0 {
                for &offset in offsets {
                    expected[offset..offset + 4].fill(0);
                }
            }
        }
        for _ in 0..2 {
            unsafe {
                ffi::apply_post_processing(actual.0.as_mut_ptr() as usize, &options);
            }
            assert_eq!(actual.0, expected, "disabled mask {disabled:#x}");
        }
    }
}

#[test]
fn old_and_partial_configs_preserve_unspecified_effects() {
    for text in ["", "[post_processing]"] {
        let config: Config = toml::from_str(text).unwrap();
        let p = config.post_processing;
        assert!(p.motion_blur && p.depth_of_field && p.bloom);
        assert!(p.film_grain && p.chromatic_aberration && p.vignette);
    }
    let config: Config = toml::from_str(
        "[hud]\nenable = false\n[movies]\nenable = false\n\
         [post_processing]\nmotion_blur = false\nvignette = false",
    )
    .unwrap();
    let p = config.post_processing;
    assert!(!p.motion_blur && !p.vignette);
    assert!(p.depth_of_field && p.bloom && p.film_grain && p.chromatic_aberration);
    assert!(!config.hud.enable && !config.movies.enable);
    assert!(toml::from_str::<Config>("[post_processing]\nmotion_blurr = false").is_err());
    assert!(toml::from_str::<Config>("[post_processing]\nbloom = 0").is_err());
}
