//! One of the four independent profile slots and global app settings.

use serde::{Deserialize, Serialize};

use crate::model::action::Action;
use crate::model::layout::{KEY_COUNT, layout};
use crate::model::lighting::Lighting;
use crate::model::macro_def::{MacroDef, RepeatMode};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    #[serde(default)]
    pub normal: Vec<Action>,
    /// FN-layer assignments.
    #[serde(default)]
    pub func: Vec<Action>,
    #[serde(default)]
    pub macros: Vec<MacroDef>,
    #[serde(default)]
    pub lighting: Lighting,
}

impl Profile {
    pub fn new(name: impl Into<String>, slot: usize) -> Self {
        let layout = layout();
        Profile {
            name: name.into(),
            normal: layout.default_normal(),
            func: layout.default_func(),
            macros: Vec::new(),
            lighting: Lighting::for_slot(slot),
        }
    }

    /// Repair loaded data: wrong lengths, out-of-range values, dangling macro refs.
    pub fn ensure_valid(&mut self) {
        let layout = layout();
        self.normal.resize(KEY_COUNT, Action::None);
        self.func.resize(KEY_COUNT, Action::None);
        if self.normal.len() != KEY_COUNT {
            self.normal = layout.default_normal();
        }
        if self.func.len() != KEY_COUNT {
            self.func = layout.default_func();
        }
        self.lighting.ensure_len(KEY_COUNT);
        for macro_def in &mut self.macros {
            macro_def.default_delay_ms = macro_def.default_delay_ms.clamp(0, 5000);
            if let RepeatMode::Count { count } = &mut macro_def.repeat {
                *count = (*count).clamp(1, 999);
            }
        }
        if self.name.trim().is_empty() {
            self.name = "Profile".into();
        }
    }

    pub fn reset_keymap(&mut self) {
        let layout = layout();
        self.normal = layout.default_normal();
        self.func = layout.default_func();
    }

    pub fn reset_all(&mut self, slot: usize) {
        let name = self.name.clone();
        *self = Profile::new(name, slot);
    }

    pub fn action(&self, layer: crate::model::Layer, index: usize) -> &Action {
        match layer {
            crate::model::Layer::Normal => &self.normal[index],
            crate::model::Layer::Fn => &self.func[index],
        }
    }

    pub fn action_mut(&mut self, layer: crate::model::Layer, index: usize) -> &mut Action {
        match layer {
            crate::model::Layer::Normal => &mut self.normal[index],
            crate::model::Layer::Fn => &mut self.func[index],
        }
    }

    pub fn next_macro_id(&self) -> u64 {
        self.macros.iter().map(|m| m.id).max().unwrap_or(0) + 1
    }

    pub fn macro_by_id(&self, id: u64) -> Option<&MacroDef> {
        self.macros.iter().find(|m| m.id == id)
    }

    pub fn macro_by_id_mut(&mut self, id: u64) -> Option<&mut MacroDef> {
        self.macros.iter_mut().find(|m| m.id == id)
    }

    /// Number of keys that differ from the stock keymap, for the UI summary.
    pub fn remap_count(&self) -> usize {
        let layout = layout();
        let default_normal = layout.default_normal();
        let default_func = layout.default_func();
        self.normal
            .iter()
            .zip(default_normal.iter())
            .filter(|(a, b)| a != b)
            .count()
            + self
                .func
                .iter()
                .zip(default_func.iter())
                .filter(|(a, b)| a != b)
                .count()
    }
}

/// What turning the rotary volume knob does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnobTurn {
    /// Send volume up/down to Windows (the firmware default).
    #[default]
    Volume,
    /// Turn the knob into LED brightness control.
    Brightness,
    /// Turn the knob into animation speed control.
    Speed,
    /// Turn the knob into lighting effect cycling.
    Effects,
}

impl KnobTurn {
    pub const ALL: [KnobTurn; 4] = [
        KnobTurn::Volume,
        KnobTurn::Brightness,
        KnobTurn::Speed,
        KnobTurn::Effects,
    ];

    pub fn label(self) -> &'static str {
        match self {
            KnobTurn::Volume => "Volume",
            KnobTurn::Brightness => "LED brightness",
            KnobTurn::Speed => "LED speed",
            KnobTurn::Effects => "Cycle effects",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// Never touch the real HID device; build and log packets only.
    #[serde(default = "default_true")]
    pub dry_run: bool,
    /// Install the low-level keyboard hook (needed for remap, macros, tester, hotkeys).
    #[serde(default = "default_true")]
    pub hook_enabled: bool,
    /// Software remapping: intercept keys and inject replacements.
    #[serde(default = "default_true")]
    pub remap_enabled: bool,
    /// Ctrl+Alt+1..4 switches profiles while the hook is active.
    #[serde(default = "default_true")]
    pub hotkeys_enabled: bool,
    /// Virtual key reported by the LK67 for its FN key. Firmware placeholder.
    #[serde(default = "default_fn_vk")]
    pub fn_key_vk: u16,
    /// What the rotary knob does when turned.
    #[serde(default)]
    pub knob_turn: KnobTurn,
    /// Paint the live RGB preview onto the keymap canvas.
    #[serde(default = "default_true")]
    pub show_keymap_rgb: bool,
    /// Rate for the live music (`0x0D`) and screen (`0x0E`) streams.
    #[serde(default = "default_stream_fps")]
    pub stream_fps: u8,
    #[serde(default = "default_true")]
    pub log_packets: bool,
    /// Ask the firmware to store the profile in its own slot when switching.
    #[serde(default = "default_true")]
    pub sync_profile_to_hardware: bool,
}

fn default_true() -> bool {
    true
}
fn default_fn_vk() -> u16 {
    crate::model::keycode::VK_FN
}
fn default_stream_fps() -> u8 {
    50
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            dry_run: true,
            hook_enabled: true,
            remap_enabled: true,
            hotkeys_enabled: true,
            fn_key_vk: default_fn_vk(),
            knob_turn: KnobTurn::Volume,
            show_keymap_rgb: true,
            stream_fps: default_stream_fps(),
            log_packets: true,
            sync_profile_to_hardware: true,
        }
    }
}

impl Settings {
    pub fn ensure_valid(&mut self) {
        self.stream_fps = self.stream_fps.clamp(20, 60);
        if self.fn_key_vk == 0 {
            self.fn_key_vk = crate::model::keycode::VK_FN;
        }
    }
}
