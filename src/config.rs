use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub super_enable: bool,
    pub hud: Hud,
    pub movies: Movies,
    pub background: Background,
    pub post_processing: PostProcessing,
    pub fonts: Fonts,
}

/// `[fonts]` -- swapping the game's font for a `.ttf` or `.otf` placed beside
/// the ASI. `file` is not read from the TOML: loading fills it with the font
/// [`find_font`] picks, and leaves it `None` when there is none.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Fonts {
    pub enable: bool,
    #[serde(skip)]
    pub file: Option<PathBuf>,
}

/// `[hud]` -- the centered UI root, and with it the tooltip positions and the
/// menu dimming that follow that root.
#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Hud {
    pub enable: bool,
    /// Preset selecting the aspect the root is held inside; see [`Hud::aspect`].
    pub constrain: u32,
}

/// `[movies]` -- fitting a movie to 16:9 and painting the bars beside it black.
#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Movies {
    pub enable: bool,
}

/// `[background]` -- keeping the game running while its window is unfocused.
#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Background {
    pub enable: bool,
}

/// `true` retains the game's effect; `false` suppresses it in each final view.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PostProcessing {
    pub motion_blur: bool,
    pub depth_of_field: bool,
    pub bloom: bool,
    pub film_grain: bool,
    pub chromatic_aberration: bool,
    pub vignette: bool,
}

impl Default for PostProcessing {
    fn default() -> Self {
        Self {
            motion_blur: true,
            depth_of_field: true,
            bloom: true,
            film_grain: true,
            chromatic_aberration: true,
            vignette: true,
        }
    }
}

impl Hud {
    /// The width-over-height ratio `constrain` selects: 0 is 16:9, 1 is 21:9,
    /// 2 is 32:9. Every other value is 16:9, which holds the root to the
    /// narrowest preset rather than leaving it uncorrected, so an out-of-range
    /// number is visible on screen instead of silently doing nothing.
    pub fn aspect(&self) -> f64 {
        match self.constrain {
            1 => 21.0 / 9.0,
            2 => 32.0 / 9.0,
            _ => 16.0 / 9.0,
        }
    }
}

// Defaults match the values the shipped TOML carries, so the config falling
// back -- missing file, unreadable directory, one mistyped key -- applies the
// same fixes the shipped file asks for rather than a quieter subset.
impl Default for Config {
    fn default() -> Self {
        Self {
            super_enable: true,
            hud: Hud::default(),
            movies: Movies::default(),
            background: Background::default(),
            post_processing: PostProcessing::default(),
            fonts: Fonts::default(),
        }
    }
}
impl Default for Hud {
    fn default() -> Self {
        Self {
            enable: true,
            constrain: 0,
        }
    }
}
impl Default for Movies {
    fn default() -> Self {
        Self { enable: true }
    }
}
impl Default for Background {
    fn default() -> Self {
        Self { enable: true }
    }
}

/// Reads `final_fantasy_resonance_fix.toml` from the same directory as
/// `this_module`, then logs whichever config ends up in effect, regardless of
/// which path below produced it. That includes the disabled-mod case:
/// otherwise, disabling the mod would be the one path that never shows what
/// the file actually contained.
pub fn load(this_module: HMODULE) -> Config {
    let config = load_inner(this_module);
    log::info!("config: {config:?}");
    config
}

/// `this_module` must be *this DLL's own* handle (`DllMain`'s first
/// parameter), not the game's. A relative filename would depend on the
/// process's current working directory -- for a launched game, that's its
/// install root, not the `scripts` folder this DLL and its config actually
/// live in.
///
/// Falls back to defaults on any error -- a missing or malformed config should
/// degrade gracefully, not stop every fix in the mod from applying.
fn load_inner(this_module: HMODULE) -> Config {
    let Some(dir) = module_dir(this_module) else {
        log::warn!("config: could not resolve this DLL's own directory, using defaults");
        return Config::default();
    };
    let path = dir.join("final_fantasy_resonance_fix.toml");

    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => {
            log::info!("config: {} not found ({e}), using defaults", path.display());
            return Config::default();
        }
    };

    match toml::from_str::<Config>(&text) {
        Ok(mut config) => {
            if config.fonts.enable {
                config.fonts.file = find_font(&dir);
            }
            log::info!("config: loaded {}", path.display());
            config
        }
        Err(e) => {
            log::warn!(
                "config: {} failed to parse ({e}), using defaults",
                path.display()
            );
            Config::default()
        }
    }
}

/// The font to swap in: the first `.ttf` or `.otf` in `dir` by name, so the
/// pick does not depend on the order the directory lists its files in.
fn find_font(dir: &Path) -> Option<PathBuf> {
    let mut fonts: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| {
            path.is_file()
                && path.extension().is_some_and(|extension| {
                    extension.eq_ignore_ascii_case("ttf") || extension.eq_ignore_ascii_case("otf")
                })
        })
        .collect();
    fonts.sort();
    if fonts.len() > 1 {
        log::warn!(
            "config: {} fonts in {}, using {}",
            fonts.len(),
            dir.display(),
            fonts[0].display()
        );
    }
    fonts.into_iter().next()
}

/// Gets the filesystem path from the module.
pub(crate) fn module_dir(module: HMODULE) -> Option<PathBuf> {
    let mut buf = [0u16; 260];
    let len = unsafe { GetModuleFileNameW(Some(module), &mut buf) } as usize;
    if len == 0 {
        return None;
    }

    PathBuf::from(String::from_utf16_lossy(&buf[..len]))
        .parent()
        .map(|p| p.to_path_buf())
}
