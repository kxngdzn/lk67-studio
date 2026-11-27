//! Macros: recorded or hand-built key sequences with per-step delays and repeat rules.

use serde::{Deserialize, Serialize};

use super::action::Mods;
use super::keycode::KeyCode;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum MacroStep {
    /// Press and keep the key down until the matching `Up` step.
    Down {
        key: KeyCode,
        #[serde(default)]
        delay_ms: u32,
    },
    /// Release a key. A release without a matching down is harmless.
    Up {
        key: KeyCode,
        #[serde(default)]
        delay_ms: u32,
    },
    /// Press and release with the default gap.
    Tap {
        key: KeyCode,
        #[serde(default)]
        delay_ms: u32,
    },
    /// Hold a modifier combo for `hold_ms`.
    Combo {
        #[serde(default)]
        mods: Mods,
        key: KeyCode,
        #[serde(default = "default_hold")]
        hold_ms: u32,
        #[serde(default)]
        delay_ms: u32,
    },
    /// Pure pause.
    Delay { delay_ms: u32 },
}

fn default_hold() -> u32 {
    30
}

impl MacroStep {
    pub fn delay(&self) -> u32 {
        match self {
            MacroStep::Down { delay_ms, .. }
            | MacroStep::Up { delay_ms, .. }
            | MacroStep::Tap { delay_ms, .. }
            | MacroStep::Combo { delay_ms, .. }
            | MacroStep::Delay { delay_ms } => *delay_ms,
        }
    }

    pub fn set_delay(&mut self, value: u32) {
        match self {
            MacroStep::Down { delay_ms, .. }
            | MacroStep::Up { delay_ms, .. }
            | MacroStep::Tap { delay_ms, .. }
            | MacroStep::Combo { delay_ms, .. }
            | MacroStep::Delay { delay_ms } => *delay_ms = value,
        }
    }

    pub fn label(&self) -> String {
        match self {
            MacroStep::Down { key, .. } => format!("Down {}", key.display()),
            MacroStep::Up { key, .. } => format!("Up {}", key.display()),
            MacroStep::Tap { key, .. } => format!("Tap {}", key.display()),
            MacroStep::Combo { mods, key, .. } => {
                if mods.any() {
                    format!("{}+{}", mods.label(), key.display())
                } else {
                    key.display()
                }
            }
            MacroStep::Delay { delay_ms } => format!("Wait {delay_ms} ms"),
        }
    }

    /// Approximate duration contribution (excluding delays after the step).
    pub fn action_ms(&self) -> u32 {
        match self {
            MacroStep::Combo { hold_ms, .. } => *hold_ms,
            _ => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum RepeatMode {
    /// Play once per press.
    #[default]
    Once,
    /// Play N times per press.
    Count {
        #[serde(default = "default_count")]
        count: u32,
    },
    /// First press starts looping, next press stops.
    Toggle,
    /// Loop while the key is held.
    Hold,
}

fn default_count() -> u32 {
    3
}

impl RepeatMode {
    pub fn label(self) -> String {
        match self {
            RepeatMode::Once => "Once".into(),
            RepeatMode::Count { count } => format!("Repeat {count}x"),
            RepeatMode::Toggle => "Toggle loop".into(),
            RepeatMode::Hold => "Loop while held".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MacroDef {
    pub id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub steps: Vec<MacroStep>,
    #[serde(default)]
    pub repeat: RepeatMode,
    /// Fallback pause for steps that have no explicit delay.
    #[serde(default = "default_step_delay")]
    pub default_delay_ms: u32,
}

fn is_modifier(vk: u16) -> bool {
    matches!(vk, 0xA0..=0xA5 | 0x5B | 0x5C)
}

pub fn default_step_delay() -> u32 {
    12
}

impl MacroDef {
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        MacroDef {
            id,
            name: name.into(),
            steps: Vec::new(),
            repeat: RepeatMode::Once,
            default_delay_ms: default_step_delay(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Compact representation for keycaps, e.g. "Ctrl+S" or "S A V E".
    pub fn summary(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        let mut held: Vec<String> = Vec::new();
        for step in &self.steps {
            match step {
                MacroStep::Down { key, .. } => held.push(key.display()),
                MacroStep::Up { key, .. } => {
                    held.pop();
                    // A recorded press/release pair reads as one key, with any
                    // modifier still held shown as a combo.
                    if !is_modifier(key.vk()) {
                        if held.is_empty() {
                            parts.push(key.display());
                        } else {
                            parts.push(format!("{}+{}", held.join("+"), key.display()));
                        }
                    }
                }
                MacroStep::Tap { key, .. } => {
                    if held.is_empty() {
                        parts.push(key.display());
                    } else {
                        parts.push(format!("{}+{}", held.join("+"), key.display()));
                    }
                }
                MacroStep::Combo { mods, key, .. } => {
                    if mods.any() {
                        parts.push(format!("{}+{}", mods.label(), key.display()));
                    } else {
                        parts.push(key.display());
                    }
                }
                MacroStep::Delay { .. } => {}
            }
        }
        if parts.is_empty() {
            "(empty)".into()
        } else {
            parts.join(" ")
        }
    }

    /// Best-effort total duration, used for the UI and playback clamping.
    pub fn duration_ms(&self) -> u32 {
        self.steps
            .iter()
            .map(|s| s.delay() + s.action_ms() + self.default_delay_ms)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_reads_like_a_combo() {
        let mut m = MacroDef::new(1, "Save as");
        m.steps.push(MacroStep::Down {
            key: KeyCode(0xA2),
            delay_ms: 0,
        });
        m.steps.push(MacroStep::Tap {
            key: KeyCode(0x53),
            delay_ms: 20,
        });
        m.steps.push(MacroStep::Up {
            key: KeyCode(0xA2),
            delay_ms: 0,
        });
        assert_eq!(m.summary(), "LCtrl+S");
    }

    #[test]
    fn summary_reads_recorded_press_and_release() {
        let mut m = MacroDef::new(2, "typed");
        for vk in [0x48, 0x49] {
            m.steps.push(MacroStep::Down {
                key: KeyCode(vk),
                delay_ms: 0,
            });
            m.steps.push(MacroStep::Up {
                key: KeyCode(vk),
                delay_ms: 10,
            });
        }
        assert_eq!(m.summary(), "H I");
    }
}
