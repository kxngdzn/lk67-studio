//! JSON persistence in `%APPDATA%\LK67 Studio\config.json`.
//!
//! Writes go through a temp file + rename so a crash mid-save cannot corrupt the
//! user's profiles.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::driver::lk67::DriverConfig;
use crate::model::layout::KEY_COUNT;
use crate::model::profile::{Profile, Settings};

pub const CONFIG_VERSION: u32 = 2;
pub const PROFILE_SLOTS: usize = 4;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub active_profile: usize,
    #[serde(default = "default_profiles")]
    pub profiles: Vec<Profile>,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub driver: DriverConfig,
}

fn default_version() -> u32 {
    CONFIG_VERSION
}

fn default_profiles() -> Vec<Profile> {
    (0..PROFILE_SLOTS)
        .map(|slot| Profile::new(format!("Profile {}", slot + 1), slot))
        .collect()
}

impl Default for Config {
    fn default() -> Self {
        Config {
            version: CONFIG_VERSION,
            active_profile: 0,
            profiles: default_profiles(),
            settings: Settings::default(),
            driver: DriverConfig::default(),
        }
    }
}

impl Config {
    pub fn profile(&self) -> &Profile {
        &self.profiles[self.active_profile.min(self.profiles.len() - 1)]
    }

    pub fn profile_mut(&mut self) -> &mut Profile {
        let index = self.active_profile.min(self.profiles.len() - 1);
        &mut self.profiles[index]
    }

    pub fn normalize(&mut self) {
        let from_version = self.version;
        self.version = CONFIG_VERSION;
        if self.profiles.len() < PROFILE_SLOTS {
            while self.profiles.len() < PROFILE_SLOTS {
                let slot = self.profiles.len();
                self.profiles
                    .push(Profile::new(format!("Profile {}", slot + 1), slot));
            }
        }
        self.profiles.truncate(PROFILE_SLOTS);
        for (slot, profile) in self.profiles.iter_mut().enumerate() {
            profile.ensure_valid();
            profile.lighting.ensure_len(KEY_COUNT);
            // Repair empty lighting sections from older/hand-edited files.
            if profile.lighting.colors.iter().all(|c| *c == [0, 0, 0]) {
                profile.lighting.colors = crate::model::lighting::default_palette();
            }
            if profile.name.trim().is_empty() {
                profile.name = format!("Profile {}", slot + 1);
            }
        }
        if self.active_profile >= PROFILE_SLOTS {
            self.active_profile = 0;
        }
        // Version 1 shipped with software remap off, so assignments appeared
        // to do nothing. Enable it once for existing installs.
        if from_version < 2 {
            self.settings.remap_enabled = true;
        }
        self.settings.ensure_valid();
        self.driver.normalize();
    }

    pub fn load() -> (Self, Vec<String>) {
        let mut notes = Vec::new();
        let path = config_path();
        let mut config = match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Config>(&text) {
                Ok(config) => {
                    notes.push(format!("Loaded {}", path.display()));
                    config
                }
                Err(err) => {
                    notes.push(format!("Could not parse {}: {err}", path.display()));
                    notes.push("Starting from defaults; the old file was left untouched.".into());
                    Config::default()
                }
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                notes.push(format!(
                    "No config yet at {}; creating defaults.",
                    path.display()
                ));
                Config::default()
            }
            Err(err) => {
                notes.push(format!("Could not read {}: {err}", path.display()));
                Config::default()
            }
        };
        config.normalize();
        (config, notes)
    }

    pub fn save(&self) -> Result<PathBuf, String> {
        let path = config_path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| format!("serialize: {e}"))?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, json).map_err(|e| format!("write {}: {e}", tmp.display()))?;
        fs::rename(&tmp, &path).map_err(|e| format!("rename onto {}: {e}", path.display()))?;
        Ok(path)
    }
}

/// `%APPDATA%\LK67 Studio` (falls back to the local config dir on other systems).
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LK67 Studio")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn config_exists() -> bool {
    Path::new(&config_path()).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trips() {
        let mut config = Config::default();
        config.active_profile = 2;
        config.profiles[2].name = "Work".into();
        config.profile_mut().lighting.brightness = 42;
        let json = serde_json::to_string_pretty(&config).unwrap();
        let mut back: Config = serde_json::from_str(&json).unwrap();
        back.normalize();
        assert_eq!(back.profiles.len(), 4);
        assert_eq!(back.profiles[2].name, "Work");
        assert_eq!(back.profiles[2].lighting.brightness, 42);
        assert_eq!(back.profiles[0].normal.len(), KEY_COUNT);
    }
}
