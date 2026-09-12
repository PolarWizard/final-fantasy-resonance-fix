use std::fs;
use std::path::PathBuf;

use serde::Deserialize;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub super_enable: bool,
    pub hud: Hud,
    pub movies: Movies,
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
        Ok(config) => {
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

#[cfg(test)]
mod tests {
    use super::{Config, Hud};

    #[test]
    fn the_shipped_config_loads_and_a_typo_does_not() {
        let current: Config =
            toml::from_str(include_str!("../final_fantasy_resonance_fix.toml")).unwrap();
        assert!(current.super_enable && current.hud.enable && current.movies.enable);
        assert_eq!(current.hud.constrain, 0);
        // deny_unknown_fields drops every other setting back to its default on
        // a single bad key, at the top level and inside a table alike.
        assert!(toml::from_str::<Config>("super_enabel = true").is_err());
        assert!(toml::from_str::<Config>("[hud]\nenabel = true").is_err());
    }

    #[test]
    fn an_omitted_table_or_key_keeps_the_shipped_defaults() {
        let config: Config = toml::from_str("super_enable = false").unwrap();
        assert!(!config.super_enable);
        assert!(config.hud.enable && config.movies.enable);
        let partial: Config = toml::from_str("[hud]\nconstrain = 2").unwrap();
        assert!(partial.hud.enable);
        assert_eq!(partial.hud.constrain, 2);
    }

    #[test]
    fn constrain_selects_a_preset_and_anything_else_is_16_9() {
        let aspect = |constrain| {
            Hud {
                enable: true,
                constrain,
            }
            .aspect()
        };
        assert_eq!(aspect(0), 16.0 / 9.0);
        assert_eq!(aspect(1), 21.0 / 9.0);
        assert_eq!(aspect(2), 32.0 / 9.0);
        for out_of_range in [3, 99, u32::MAX] {
            assert_eq!(aspect(out_of_range), 16.0 / 9.0);
        }
    }
}
