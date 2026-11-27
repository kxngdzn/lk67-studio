//! Logical keys, represented by their Windows virtual-key code.
//!
//! The LK67 firmware speaks in HID usage IDs / matrix indices; we keep everything in
//! the app as VK codes and translate at the driver boundary (`driver::lk67`). VK 0xFF
//! is reserved for the dedicated FN key, which has no standard Windows code.

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Value used for the (non-standard) dedicated FN key.
pub const VK_FN: u16 = 0xFF;

/// Every key that can be picked for an assignment, macro step or combo.
///
/// Ordered roughly like a keyboard so the picker reads naturally.
pub const KEY_TABLE: &[(u16, &str)] = &[
    (0x1B, "Esc"),
    (0x31, "1"),
    (0x32, "2"),
    (0x33, "3"),
    (0x34, "4"),
    (0x35, "5"),
    (0x36, "6"),
    (0x37, "7"),
    (0x38, "8"),
    (0x39, "9"),
    (0x30, "0"),
    (0xBD, "Minus"),
    (0xBB, "Equal"),
    (0x08, "Backspace"),
    (0x2E, "Delete"),
    (0x09, "Tab"),
    (0x51, "Q"),
    (0x57, "W"),
    (0x45, "E"),
    (0x52, "R"),
    (0x54, "T"),
    (0x59, "Y"),
    (0x55, "U"),
    (0x49, "I"),
    (0x4F, "O"),
    (0x50, "P"),
    (0xDB, "LBracket"),
    (0xDD, "RBracket"),
    (0xDC, "Backslash"),
    (0x21, "PageUp"),
    (0x14, "CapsLock"),
    (0x41, "A"),
    (0x53, "S"),
    (0x44, "D"),
    (0x46, "F"),
    (0x47, "G"),
    (0x48, "H"),
    (0x4A, "J"),
    (0x4B, "K"),
    (0x4C, "L"),
    (0xBA, "Semicolon"),
    (0xDE, "Quote"),
    (0x0D, "Enter"),
    (0x22, "PageDown"),
    (0xA0, "LShift"),
    (0x5A, "Z"),
    (0x58, "X"),
    (0x43, "C"),
    (0x56, "V"),
    (0x42, "B"),
    (0x4E, "N"),
    (0x4D, "M"),
    (0xBC, "Comma"),
    (0xBE, "Period"),
    (0xBF, "Slash"),
    (0xA1, "RShift"),
    (0x26, "Up"),
    (0x23, "End"),
    (0xA2, "LCtrl"),
    (0x5B, "LWin"),
    (0xA4, "LAlt"),
    (0x20, "Space"),
    (0xA5, "RAlt"),
    (0xFF, "Fn"),
    (0xA3, "RCtrl"),
    (0x25, "Left"),
    (0x28, "Down"),
    (0x27, "Right"),
    (0x5C, "RWin"),
    (0x5D, "Menu"),
    (0x24, "Home"),
    (0x2D, "Insert"),
    (0x2C, "PrintScreen"),
    (0x13, "Pause"),
    (0x90, "NumLock"),
    (0x91, "ScrollLock"),
    (0xC0, "Grave"),
    (0xE2, "Oem102"),
    (0xAD, "Mute"),
    (0xAE, "VolDown"),
    (0xAF, "VolUp"),
    (0xB0, "MediaNext"),
    (0xB1, "MediaPrev"),
    (0xB2, "MediaStop"),
    (0xB3, "PlayPause"),
    (0x70, "F1"),
    (0x71, "F2"),
    (0x72, "F3"),
    (0x73, "F4"),
    (0x74, "F5"),
    (0x75, "F6"),
    (0x76, "F7"),
    (0x77, "F8"),
    (0x78, "F9"),
    (0x79, "F10"),
    (0x7A, "F11"),
    (0x7B, "F12"),
    (0x7C, "F13"),
    (0x7D, "F14"),
    (0x7E, "F15"),
    (0x7F, "F16"),
    (0x80, "F17"),
    (0x81, "F18"),
    (0x82, "F19"),
    (0x83, "F20"),
    (0x84, "F21"),
    (0x85, "F22"),
    (0x86, "F23"),
    (0x87, "F24"),
    (0x60, "Num0"),
    (0x61, "Num1"),
    (0x62, "Num2"),
    (0x63, "Num3"),
    (0x64, "Num4"),
    (0x65, "Num5"),
    (0x66, "Num6"),
    (0x67, "Num7"),
    (0x68, "Num8"),
    (0x69, "Num9"),
    (0x6A, "NumMultiply"),
    (0x6B, "NumAdd"),
    (0x6D, "NumSubtract"),
    (0x6E, "NumDecimal"),
    (0x6F, "NumDivide"),
];

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct KeyCode(pub u16);

impl KeyCode {
    pub const NONE: Self = KeyCode(0);
    pub const FN: Self = KeyCode(VK_FN);

    pub const fn vk(self) -> u16 {
        self.0
    }

    pub const fn is_none(self) -> bool {
        self.0 == 0
    }

    /// Canonical, stable name used in JSON and the UI.
    pub fn name(self) -> String {
        if self.0 == 0 {
            return "None".to_string();
        }
        if let Some((_, name)) = KEY_TABLE.iter().find(|(vk, _)| *vk == self.0) {
            return (*name).to_string();
        }
        format!("VK_{:02X}", self.0)
    }

    /// Compact label for keycaps and tables.
    pub fn display(self) -> String {
        if self.0 == 0 {
            return "--".to_string();
        }
        let name = self.name();
        match name.as_str() {
            "PageUp" => "PgUp".into(),
            "PageDown" => "PgDn".into(),
            "Backspace" => "Bksp".into(),
            "CapsLock" => "Caps".into(),
            "LShift" => "LShft".into(),
            "RShift" => "RShft".into(),
            "LCtrl" => "LCtrl".into(),
            "RCtrl" => "RCtrl".into(),
            "LAlt" => "LAlt".into(),
            "RAlt" => "RAlt".into(),
            "LWin" => "LWin".into(),
            "RWin" => "RWin".into(),
            "PrintScreen" => "PrtSc".into(),
            "NumLock" => "Num".into(),
            "ScrollLock" => "ScrLk".into(),
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
            "Oem102" => "\\".into(),
            "Delete" => "Del".into(),
            "Insert" => "Ins".into(),
            "PlayPause" => "Play".into(),
            "MediaNext" => "Next".into(),
            "MediaPrev" => "Prev".into(),
            "MediaStop" => "Stop".into(),
            "NumMultiply" => "Num*".into(),
            "NumAdd" => "Num+".into(),
            "NumSubtract" => "Num-".into(),
            "NumDecimal" => "Num.".into(),
            "NumDivide" => "Num/".into(),
            "Space" => "Space".into(),
            other => other.to_string(),
        }
    }

    /// Parse a name from JSON / user input. Unknown names become `None`.
    pub fn from_name(input: &str) -> Option<Self> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Some(KeyCode::NONE);
        }
        let lower = trimmed.to_ascii_lowercase();
        let alias = match lower.as_str() {
            "none" | "null" | "--" => Some(KeyCode::NONE),
            "escape" => Some(KeyCode(0x1B)),
            "return" => Some(KeyCode(0x0D)),
            "del" => Some(KeyCode(0x2E)),
            "ins" => Some(KeyCode(0x2D)),
            "bksp" | "back" => Some(KeyCode(0x08)),
            "pgup" => Some(KeyCode(0x21)),
            "pgdn" => Some(KeyCode(0x22)),
            "ctrl" | "control" => Some(KeyCode(0xA2)),
            "shift" => Some(KeyCode(0xA0)),
            "alt" => Some(KeyCode(0xA4)),
            "win" => Some(KeyCode(0x5B)),
            "spacebar" => Some(KeyCode(0x20)),
            "app" | "apps" => Some(KeyCode(0x5D)),
            "play" => Some(KeyCode(0xB3)),
            "volumeup" | "vol+" => Some(KeyCode(0xAF)),
            "volumedown" | "vol-" => Some(KeyCode(0xAE)),
            "fn" => Some(KeyCode::FN),
            _ => None,
        };
        if let Some(key) = alias {
            return Some(key);
        }
        if let Some((vk, _)) = KEY_TABLE
            .iter()
            .find(|(_, name)| name.eq_ignore_ascii_case(trimmed))
        {
            return Some(KeyCode(*vk));
        }
        // Single character shortcuts: "A", "7", ";".
        let mut chars = trimmed.chars();
        if let (Some(ch), None) = (chars.next(), chars.next()) {
            let upper = ch.to_ascii_uppercase();
            if upper.is_ascii_alphanumeric() {
                return Some(KeyCode(upper as u16));
            }
            let punct = match ch {
                ';' => Some(0xBA),
                '=' => Some(0xBB),
                ',' => Some(0xBC),
                '-' => Some(0xBD),
                '.' => Some(0xBE),
                '/' => Some(0xBF),
                '`' => Some(0xC0),
                '[' => Some(0xDB),
                '\\' => Some(0xDC),
                ']' => Some(0xDD),
                '\'' => Some(0xDE),
                _ => None,
            };
            if let Some(vk) = punct {
                return Some(KeyCode(vk));
            }
        }
        // Raw "VK_XX" / "0xXX" escape hatch.
        let hex = lower
            .strip_prefix("vk_")
            .or_else(|| lower.strip_prefix("0x"));
        if let Some(hex) = hex {
            if let Ok(vk) = u16::from_str_radix(hex, 16) {
                return Some(KeyCode(vk));
            }
        }
        None
    }
}

impl fmt::Debug for KeyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl fmt::Display for KeyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name())
    }
}

impl Serialize for KeyCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.name())
    }
}

impl<'de> Deserialize<'de> for KeyCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct KeyVisitor;

        impl Visitor<'_> for KeyVisitor {
            type Value = KeyCode;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a key name such as \"Esc\" or a raw VK code")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(KeyCode::from_name(value).unwrap_or(KeyCode::NONE))
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(KeyCode(value as u16))
            }
        }

        deserializer.deserialize_any(KeyVisitor)
    }
}

impl From<u16> for KeyCode {
    fn from(vk: u16) -> Self {
        KeyCode(vk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for (vk, name) in KEY_TABLE {
            let key = KeyCode(*vk);
            assert_eq!(key.name(), *name);
            assert_eq!(KeyCode::from_name(name), Some(key));
        }
        assert_eq!(KeyCode::from_name("esc"), Some(KeyCode(0x1B)));
        assert_eq!(KeyCode::from_name("Play"), Some(KeyCode(0xB3)));
        assert_eq!(KeyCode::from_name("VK_42"), Some(KeyCode(0x42)));
        assert_eq!(KeyCode::from_name("A"), Some(KeyCode(0x41)));
    }

    #[test]
    fn json_uses_names() {
        let json = serde_json::to_string(&KeyCode(0x41)).unwrap();
        assert_eq!(json, "\"A\"");
    }
}
