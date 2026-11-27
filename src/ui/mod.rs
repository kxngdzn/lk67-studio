//! egui front-end: the keymap canvas, navigation shell and one module per page.

pub mod driver;
pub mod keymap;
pub mod lighting;
pub mod macros;
pub mod music;
pub mod remap;
pub mod screen;
pub mod settings;
pub mod tester;
pub mod theme;
pub mod widgets;

use std::time::Instant;

use crate::model::action::{MediaKey, Mods, RgbCommand};
use crate::model::keycode::KeyCode;
use crate::model::layout::KEY_COUNT;

use crate::model::Layer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Remap,
    Lighting,
    Macros,
    Tester,
    Music,
    Screen,
    Driver,
    Settings,
}

impl Page {
    pub const ALL: [Page; 8] = [
        Page::Remap,
        Page::Lighting,
        Page::Macros,
        Page::Tester,
        Page::Music,
        Page::Screen,
        Page::Driver,
        Page::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Page::Remap => "Remap & keymap",
            Page::Lighting => "Lighting",
            Page::Macros => "Macros",
            Page::Tester => "Key tester",
            Page::Music => "Music reactive",
            Page::Screen => "Screen follow",
            Page::Driver => "Driver & packets",
            Page::Settings => "Settings",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Page::Remap => "Assign keys, combos and macros on both layers.",
            Page::Lighting => "13 hardware effects, 14-colour palette and custom per-key layouts.",
            Page::Macros => "Record, edit and test key sequences with delays and repeats.",
            Page::Tester => {
                "Live switch tester: press counts, hold times and a heatmap from real keys."
            }
            Page::Music => "WASAPI loopback or microphone driven by FFT band analysis.",
            Page::Screen => "Map screen colours onto the keyboard (ambilight).",
            Page::Driver => "HID discovery, protocol placeholders and the packet log.",
            Page::Settings => "Runtime options, FN key and software remapping.",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Paint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    None,
    Key,
    Combo,
    Macro,
    Media,
    Fn,
    Rgb,
    Profile,
}

impl ActionKind {
    pub const ALL: [ActionKind; 8] = [
        ActionKind::Key,
        ActionKind::Combo,
        ActionKind::Macro,
        ActionKind::Media,
        ActionKind::Rgb,
        ActionKind::Profile,
        ActionKind::Fn,
        ActionKind::None,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ActionKind::None => "Disabled",
            ActionKind::Key => "Single key",
            ActionKind::Combo => "Combo",
            ActionKind::Macro => "Macro",
            ActionKind::Media => "Media key",
            ActionKind::Fn => "FN layer",
            ActionKind::Rgb => "Lighting command",
            ActionKind::Profile => "Switch profile",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepKindChoice {
    Down,
    Up,
    Tap,
    Combo,
}

impl StepKindChoice {
    pub const ALL: [StepKindChoice; 4] = [
        StepKindChoice::Tap,
        StepKindChoice::Down,
        StepKindChoice::Up,
        StepKindChoice::Combo,
    ];

    pub fn label(self) -> &'static str {
        match self {
            StepKindChoice::Down => "Key down",
            StepKindChoice::Up => "Key up",
            StepKindChoice::Tap => "Tap",
            StepKindChoice::Combo => "Combo",
        }
    }
}

pub struct UiState {
    pub page: Page,
    pub layer: Layer,
    pub tool: Tool,
    pub selected: Option<usize>,
    /// Palette slot used by the paint tool.
    pub palette_index: usize,
    pub paint_color: [u8; 3],

    pub assign_kind: ActionKind,
    pub assign_key: KeyCode,
    pub combo_mods: Mods,
    pub combo_key: KeyCode,
    pub assign_macro: u64,
    pub assign_media: MediaKey,
    pub assign_rgb: RgbCommand,
    pub assign_slot: u8,

    pub status: String,
    pub status_at: Option<Instant>,

    pub selected_macro: Option<u64>,
    pub new_macro_name: String,
    pub step_kind: StepKindChoice,
    pub step_key: KeyCode,
    pub step_delay: u32,

    pub preset_name: String,
    pub anim_name: String,
    pub stream_to_device: bool,

    pub audio_requested: bool,
    pub screen_requested: bool,

    pub packet_hex: bool,

    pub tester_counts: Vec<u32>,
    pub tester_last: Vec<Option<Instant>>,
    pub tester_hold_ms: Vec<u32>,
    pub tester_down_at: Vec<Option<Instant>>,
    pub tester_total: u64,
    pub tester_heatmap: bool,

    /// Last time the knob press (0xAD/0xB3) or its rotation (0xAE/0xAF) was seen.
    pub knob_press_at: Option<Instant>,
    pub knob_turn_at: Option<Instant>,

    pub fn_held: bool,
    pub dirty: bool,
    pub apply_lighting: bool,
    pub apply_keymap: bool,
    /// Set when a lighting widget changed; the app debounces it into a write so
    /// colour edits reach the keyboard as soon as the user stops dragging.
    pub lighting_edit_at: Option<Instant>,
    pub restoring: bool,
    /// A background per-key picture upload is running.
    pub lighting_uploading: bool,
    pub confirm_reset: bool,
}

impl Default for UiState {
    fn default() -> Self {
        UiState {
            page: Page::Remap,
            layer: Layer::Normal,
            tool: Tool::Select,
            selected: None,
            palette_index: 0,
            paint_color: [255, 60, 60],
            assign_kind: ActionKind::Key,
            assign_key: KeyCode::NONE,
            combo_mods: Mods::default(),
            combo_key: KeyCode::NONE,
            assign_macro: 0,
            assign_media: MediaKey::PlayPause,
            assign_rgb: RgbCommand::NextEffect,
            assign_slot: 0,
            status: "Ready".into(),
            status_at: None,
            selected_macro: None,
            new_macro_name: String::new(),
            step_kind: StepKindChoice::Tap,
            step_key: KeyCode(0x41),
            step_delay: 40,
            preset_name: String::new(),
            anim_name: String::new(),
            stream_to_device: true,
            audio_requested: false,
            screen_requested: false,
            packet_hex: true,
            tester_counts: vec![0; KEY_COUNT],
            tester_last: vec![None; KEY_COUNT],
            tester_hold_ms: vec![0; KEY_COUNT],
            tester_down_at: vec![None; KEY_COUNT],
            tester_total: 0,
            tester_heatmap: true,
            knob_press_at: None,
            knob_turn_at: None,
            fn_held: false,
            dirty: false,
            apply_lighting: false,
            apply_keymap: false,
            lighting_edit_at: None,
            restoring: false,
            lighting_uploading: false,
            confirm_reset: false,
        }
    }
}

impl UiState {
    pub fn notify(&mut self, message: impl Into<String>) {
        self.status = message.into();
        self.status_at = Some(Instant::now());
    }

    pub fn reset_tester(&mut self) {
        self.tester_counts = vec![0; KEY_COUNT];
        self.tester_last = vec![None; KEY_COUNT];
        self.tester_hold_ms = vec![0; KEY_COUNT];
        self.tester_down_at = vec![None; KEY_COUNT];
        self.tester_total = 0;
    }
}
