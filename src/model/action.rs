//! What a physical key does on a given layer.

use serde::{Deserialize, Serialize};

use super::keycode::KeyCode;
use super::macro_def::MacroDef;

/// Modifier bitmask used by combos and macro steps (1=Ctrl, 2=Shift, 4=Alt, 8=Win).
pub const MOD_CTRL: u8 = 1;
pub const MOD_SHIFT: u8 = 2;
pub const MOD_ALT: u8 = 4;
pub const MOD_WIN: u8 = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mods {
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub win: bool,
}

impl Mods {
    pub fn any(self) -> bool {
        self.ctrl || self.shift || self.alt || self.win
    }

    pub fn bits(self) -> u8 {
        (if self.ctrl { MOD_CTRL } else { 0 })
            | (if self.shift { MOD_SHIFT } else { 0 })
            | (if self.alt { MOD_ALT } else { 0 })
            | (if self.win { MOD_WIN } else { 0 })
    }

    pub fn label(self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.win {
            parts.push("Win");
        }
        parts.join("+")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKey {
    PlayPause,
    Next,
    Prev,
    Stop,
    Mute,
    VolumeUp,
    VolumeDown,
    Calculator,
    Email,
    Browser,
}

impl MediaKey {
    pub fn vk(self) -> u16 {
        match self {
            MediaKey::PlayPause => 0xB3,
            MediaKey::Next => 0xB0,
            MediaKey::Prev => 0xB1,
            MediaKey::Stop => 0xB2,
            MediaKey::Mute => 0xAD,
            MediaKey::VolumeUp => 0xAF,
            MediaKey::VolumeDown => 0xAE,
            MediaKey::Calculator => 0x81, // VK_LAUNCH_APP2
            MediaKey::Email => 0x86,
            MediaKey::Browser => 0xAC,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            MediaKey::PlayPause => "Play/Pause",
            MediaKey::Next => "Next track",
            MediaKey::Prev => "Previous track",
            MediaKey::Stop => "Stop",
            MediaKey::Mute => "Mute",
            MediaKey::VolumeUp => "Volume up",
            MediaKey::VolumeDown => "Volume down",
            MediaKey::Calculator => "Calculator",
            MediaKey::Email => "E-mail",
            MediaKey::Browser => "Browser",
        }
    }

    pub const ALL: [MediaKey; 10] = [
        MediaKey::PlayPause,
        MediaKey::Next,
        MediaKey::Prev,
        MediaKey::Stop,
        MediaKey::Mute,
        MediaKey::VolumeUp,
        MediaKey::VolumeDown,
        MediaKey::Calculator,
        MediaKey::Email,
        MediaKey::Browser,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RgbCommand {
    NextEffect,
    PrevEffect,
    BrightnessUp,
    BrightnessDown,
    SpeedUp,
    SpeedDown,
    Toggle,
    NextProfile,
}

impl RgbCommand {
    pub fn label(self) -> &'static str {
        match self {
            RgbCommand::NextEffect => "Next lighting effect",
            RgbCommand::PrevEffect => "Previous lighting effect",
            RgbCommand::BrightnessUp => "Brightness up",
            RgbCommand::BrightnessDown => "Brightness down",
            RgbCommand::SpeedUp => "Speed up",
            RgbCommand::SpeedDown => "Slow down",
            RgbCommand::Toggle => "Toggle lighting",
            RgbCommand::NextProfile => "Next profile",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            RgbCommand::NextEffect => "FX+",
            RgbCommand::PrevEffect => "FX-",
            RgbCommand::BrightnessUp => "Bri+",
            RgbCommand::BrightnessDown => "Bri-",
            RgbCommand::SpeedUp => "Spd+",
            RgbCommand::SpeedDown => "Spd-",
            RgbCommand::Toggle => "LED",
            RgbCommand::NextProfile => "Prof+",
        }
    }

    pub const ALL: [RgbCommand; 8] = [
        RgbCommand::NextEffect,
        RgbCommand::PrevEffect,
        RgbCommand::BrightnessUp,
        RgbCommand::BrightnessDown,
        RgbCommand::SpeedUp,
        RgbCommand::SpeedDown,
        RgbCommand::Toggle,
        RgbCommand::NextProfile,
    ];
}

/// A complete key assignment.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Key does nothing (swallowed).
    #[default]
    None,
    /// Type another key.
    Key { key: KeyCode },
    /// Type a modifier combo such as Ctrl+Shift+Esc.
    Combo {
        #[serde(default)]
        mods: Mods,
        key: KeyCode,
    },
    /// Play a macro stored in the same profile.
    Macro { macro_id: u64 },
    /// A media / system key.
    Media { media: MediaKey },
    /// Momentary FN layer while held.
    Fn,
    /// Control the lighting directly from the keyboard.
    Rgb { command: RgbCommand },
    /// Switch to another profile slot (0-based).
    Profile { slot: u8 },
}

impl Action {
    pub fn short(&self, macros: &[MacroDef]) -> String {
        match self {
            Action::None => "--".into(),
            Action::Key { key } => key.display(),
            Action::Combo { mods, key } => {
                if mods.any() {
                    format!("{}+{}", mods.label(), key.display())
                } else {
                    key.display()
                }
            }
            Action::Macro { macro_id } => macros
                .iter()
                .find(|m| m.id == *macro_id)
                .map(|m| format!("M:{}", truncate(&m.name, 8)))
                .unwrap_or_else(|| format!("M#{macro_id}")),
            Action::Media { media } => match media {
                MediaKey::PlayPause => "Play".into(),
                MediaKey::Next => "Next".into(),
                MediaKey::Prev => "Prev".into(),
                MediaKey::Mute => "Mute".into(),
                MediaKey::VolumeUp => "Vol+".into(),
                MediaKey::VolumeDown => "Vol-".into(),
                MediaKey::Stop => "Stop".into(),
                MediaKey::Calculator => "Calc".into(),
                MediaKey::Email => "Mail".into(),
                MediaKey::Browser => "Web".into(),
            },
            Action::Fn => "FN".into(),
            Action::Rgb { command } => command.short().into(),
            Action::Profile { slot } => format!("P{}", slot + 1),
        }
    }

    pub fn summary(&self, macros: &[MacroDef]) -> String {
        match self {
            Action::None => "Disabled (swallowed)".into(),
            Action::Key { key } => format!("Key {}", key.display()),
            Action::Combo { mods, key } => {
                if mods.any() {
                    format!("Combo {}+{}", mods.label(), key.display())
                } else {
                    format!("Key {}", key.display())
                }
            }
            Action::Macro { macro_id } => macros
                .iter()
                .find(|m| m.id == *macro_id)
                .map(|m| format!("Macro \u{201c}{}\u{201d}", m.name))
                .unwrap_or_else(|| format!("Macro #{macro_id} (missing)")),
            Action::Media { media } => format!("Media: {}", media.label()),
            Action::Fn => "Momentary FN layer".into(),
            Action::Rgb { command } => format!("Lighting: {}", command.label()),
            Action::Profile { slot } => format!("Switch to profile {}", slot + 1),
        }
    }

    /// True when the action is not a plain "this key types itself".
    pub fn is_remapped(&self, physical_vk: u16) -> bool {
        match self {
            Action::Key { key } => key.vk() != physical_vk,
            Action::Fn => physical_vk != 0xFF,
            _ => true,
        }
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        text.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
    }
}
