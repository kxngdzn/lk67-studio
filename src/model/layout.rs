//! The LK67 65% layout: 68 keys on a 16-unit grid (5 rows).
//!
//! Key indices are the app's stable identity for LEDs, matrix slots and saved keymaps.
//! Several indices are firmware placeholders (see `matrix`, `row`, `col`); the physical
//! geometry is what the UI draws and what the user sees, so keep those stable.

use std::sync::OnceLock;

use crate::model::action::{Action, MediaKey, RgbCommand};
use crate::model::keycode::KeyCode;

pub const ROWS: usize = 5;
pub const COLS: usize = 16;
pub const KEY_COUNT: usize = 68;
pub const UNITS_WIDE: f32 = 16.0;

#[derive(Clone, Copy)]
struct Spec {
    code: &'static str,
    width: f32,
    vk: u16,
}

const fn s(code: &'static str, width: f32, vk: u16) -> Spec {
    Spec { code, width, vk }
}

/// ANSI 65%: arrow cluster bottom-right, navigation column on the right edge.
const ROW_0: &[Spec] = &[
    s("Esc", 1.0, 0x1B),
    s("1", 1.0, 0x31),
    s("2", 1.0, 0x32),
    s("3", 1.0, 0x33),
    s("4", 1.0, 0x34),
    s("5", 1.0, 0x35),
    s("6", 1.0, 0x36),
    s("7", 1.0, 0x37),
    s("8", 1.0, 0x38),
    s("9", 1.0, 0x39),
    s("0", 1.0, 0x30),
    s("Minus", 1.0, 0xBD),
    s("Equal", 1.0, 0xBB),
    s("Backspace", 2.0, 0x08),
    s("Delete", 1.0, 0x2E),
];

const ROW_1: &[Spec] = &[
    s("Tab", 1.5, 0x09),
    s("Q", 1.0, 0x51),
    s("W", 1.0, 0x57),
    s("E", 1.0, 0x45),
    s("R", 1.0, 0x52),
    s("T", 1.0, 0x54),
    s("Y", 1.0, 0x59),
    s("U", 1.0, 0x55),
    s("I", 1.0, 0x49),
    s("O", 1.0, 0x4F),
    s("P", 1.0, 0x50),
    s("LBracket", 1.0, 0xDB),
    s("RBracket", 1.0, 0xDD),
    s("Backslash", 1.5, 0xDC),
    s("PageUp", 1.0, 0x21),
];

const ROW_2: &[Spec] = &[
    s("CapsLock", 1.75, 0x14),
    s("A", 1.0, 0x41),
    s("S", 1.0, 0x53),
    s("D", 1.0, 0x44),
    s("F", 1.0, 0x46),
    s("G", 1.0, 0x47),
    s("H", 1.0, 0x48),
    s("J", 1.0, 0x4A),
    s("K", 1.0, 0x4B),
    s("L", 1.0, 0x4C),
    s("Semicolon", 1.0, 0xBA),
    s("Quote", 1.0, 0xDE),
    s("Enter", 2.25, 0x0D),
    s("PageDown", 1.0, 0x22),
];

const ROW_3: &[Spec] = &[
    s("LShift", 2.25, 0xA0),
    s("Z", 1.0, 0x5A),
    s("X", 1.0, 0x58),
    s("C", 1.0, 0x43),
    s("V", 1.0, 0x56),
    s("B", 1.0, 0x42),
    s("N", 1.0, 0x4E),
    s("M", 1.0, 0x4D),
    s("Comma", 1.0, 0xBC),
    s("Period", 1.0, 0xBE),
    s("Slash", 1.0, 0xBF),
    s("RShift", 1.75, 0xA1),
    s("Up", 1.0, 0x26),
    // The LK67 has a rotary knob here, not an End key.
    s("Knob", 1.0, 0xB3),
];

const ROW_4: &[Spec] = &[
    s("LCtrl", 1.25, 0xA2),
    s("LWin", 1.25, 0x5B),
    s("LAlt", 1.25, 0xA4),
    s("Space", 6.25, 0x20),
    s("RAlt", 1.0, 0xA5),
    s("Fn", 1.0, 0xFF),
    s("RCtrl", 1.0, 0xA3),
    s("Left", 1.0, 0x25),
    s("Down", 1.0, 0x28),
    s("Right", 1.0, 0x27),
];

#[derive(Clone, Debug)]
pub struct KeyDef {
    /// Stable index: LED index, keymap slot and UI identity.
    pub index: usize,
    pub code: &'static str,
    pub vk: u16,
    /// Position/size in key units (1 unit = one 1u keycap).
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub row: u8,
    pub col: u8,
    /// Placeholder firmware matrix slot. Adjust with `driver::lk67::DriverConfig`.
    pub matrix: u16,
}

#[derive(Clone, Debug)]
pub struct Layout {
    pub keys: Vec<KeyDef>,
    pub width: f32,
    pub height: f32,
}

impl Layout {
    fn build() -> Self {
        let mut keys = Vec::with_capacity(KEY_COUNT);
        let rows: [&[Spec]; ROWS] = [ROW_0, ROW_1, ROW_2, ROW_3, ROW_4];
        for (row, specs) in rows.iter().enumerate() {
            let mut x = 0.0f32;
            for (col, spec) in specs.iter().enumerate() {
                keys.push(KeyDef {
                    index: keys.len(),
                    code: spec.code,
                    vk: spec.vk,
                    x,
                    y: row as f32,
                    w: spec.width,
                    h: 1.0,
                    row: row as u8,
                    col: col as u8,
                    // 16-column matrix is a placeholder for the real LK67 wiring.
                    matrix: (row as u16) * COLS as u16 + col as u16,
                });
                x += spec.width;
            }
        }
        debug_assert_eq!(keys.len(), KEY_COUNT);
        Layout {
            keys,
            width: UNITS_WIDE,
            height: ROWS as f32,
        }
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn key(&self, index: usize) -> &KeyDef {
        &self.keys[index]
    }

    pub fn index_of_vk(&self, vk: u16) -> Option<usize> {
        self.keys.iter().position(|k| k.vk == vk)
    }

    /// Default assignments for the Normal layer: every key types itself.
    pub fn default_normal(&self) -> Vec<Action> {
        self.keys
            .iter()
            .map(|k| {
                if k.vk == 0xFF {
                    Action::Fn
                } else if k.vk == 0xB3 {
                    Action::Media {
                        media: MediaKey::PlayPause,
                    }
                } else {
                    Action::Key { key: KeyCode(k.vk) }
                }
            })
            .collect()
    }

    /// Sensible FN layer: function row, navigation, media and a couple of RGB shortcuts.
    pub fn default_func(&self) -> Vec<Action> {
        let mut actions = self.default_normal();
        for (i, key) in self.keys.iter().enumerate() {
            let action = match key.code {
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "0" => {
                    let n = if key.code == "0" {
                        10
                    } else {
                        key.code.parse::<u16>().unwrap_or(1)
                    };
                    Action::Key {
                        key: KeyCode(0x70 + (n - 1)),
                    }
                }
                "Minus" => Action::Key { key: KeyCode(0x7A) },
                "Equal" => Action::Key { key: KeyCode(0x7B) },
                "Backspace" => Action::Key { key: KeyCode(0x2E) },
                "Delete" => Action::Key { key: KeyCode(0x2D) },
                "PageUp" => Action::Key { key: KeyCode(0x24) },
                "PageDown" => Action::Key { key: KeyCode(0x23) },
                "Up" => Action::Key { key: KeyCode(0x21) },
                "Down" => Action::Key { key: KeyCode(0x22) },
                "Left" => Action::Key { key: KeyCode(0x24) },
                "Right" => Action::Key { key: KeyCode(0x23) },
                "M" => Action::Media {
                    media: MediaKey::Mute,
                },
                "Comma" => Action::Media {
                    media: MediaKey::VolumeDown,
                },
                "Period" => Action::Media {
                    media: MediaKey::VolumeUp,
                },
                "Slash" => Action::Media {
                    media: MediaKey::PlayPause,
                },
                "LBracket" => Action::Media {
                    media: MediaKey::Prev,
                },
                "RBracket" => Action::Media {
                    media: MediaKey::Next,
                },
                "Space" => Action::Rgb {
                    command: RgbCommand::NextEffect,
                },
                "Fn" => Action::Fn,
                _ => continue,
            };
            actions[i] = action;
        }
        actions
    }

    /// Human label used on the keycap and in lists ("PageUp" -> "PgUp").
    pub fn label(&self, index: usize) -> String {
        match self.keys[index].code {
            "PageUp" => "PgUp".into(),
            "PageDown" => "PgDn".into(),
            "Backspace" => "Bksp".into(),
            "CapsLock" => "Caps".into(),
            "LShift" => "Shift".into(),
            "RShift" => "Shift".into(),
            "Semicolon" => ";".into(),
            "Quote" => "'".into(),
            "Comma" => ",".into(),
            "Period" => ".".into(),
            "Slash" => "/".into(),
            "Backslash" => "\\".into(),
            "LBracket" => "[".into(),
            "RBracket" => "]".into(),
            "Grave" => "`".into(),
            "Minus" => "-".into(),
            "Equal" => "=".into(),
            "Delete" => "Del".into(),
            other => other.to_string(),
        }
    }
}

pub fn layout() -> &'static Layout {
    static LAYOUT: OnceLock<Layout> = OnceLock::new();
    LAYOUT.get_or_init(Layout::build)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_68_keys_on_16_units() {
        let layout = layout();
        assert_eq!(layout.len(), 68);
        for (row, start_end) in [
            (0usize, ROW_0),
            (1, ROW_1),
            (2, ROW_2),
            (3, ROW_3),
            (4, ROW_4),
        ] {
            let total: f32 = start_end.iter().map(|s| s.width).sum();
            assert!(
                (total - 16.0).abs() < f32::EPSILON,
                "row {row} is {total} units"
            );
        }
        assert_eq!(layout.default_normal().len(), KEY_COUNT);
        assert_eq!(layout.default_func().len(), KEY_COUNT);
    }
}
