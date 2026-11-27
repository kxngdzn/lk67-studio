//! Lighting configuration: the 13 hardware effect slots, the 14-colour palette,
//! per-key custom layouts, keyframe animations, music and screen-follow settings.

use serde::{Deserialize, Serialize};

use crate::model::layout::KEY_COUNT;

pub const PALETTE_SLOTS: usize = 14;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    #[default]
    Right,
    Left,
    Up,
    Down,
    Clockwise,
    CounterClockwise,
}

impl Direction {
    pub const ALL: [Direction; 6] = [
        Direction::Right,
        Direction::Left,
        Direction::Up,
        Direction::Down,
        Direction::Clockwise,
        Direction::CounterClockwise,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Direction::Right => "Left to right",
            Direction::Left => "Right to left",
            Direction::Up => "Bottom to top",
            Direction::Down => "Top to bottom",
            Direction::Clockwise => "Clockwise",
            Direction::CounterClockwise => "Counter-clockwise",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LightingMode {
    /// Let the keyboard firmware render one of its 13 effects.
    #[default]
    Hardware,
    /// Per-key colours painted here (static or animated).
    Custom,
    /// Audio reactive.
    Music,
    /// Screen ambilight.
    Screen,
}

impl LightingMode {
    pub const ALL: [LightingMode; 4] = [
        LightingMode::Hardware,
        LightingMode::Custom,
        LightingMode::Music,
        LightingMode::Screen,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LightingMode::Hardware => "Hardware effects",
            LightingMode::Custom => "Custom per-key",
            LightingMode::Music => "Music reactive",
            LightingMode::Screen => "Screen follow",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            LightingMode::Hardware => "Hardware",
            LightingMode::Custom => "Custom",
            LightingMode::Music => "Music",
            LightingMode::Screen => "Screen",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EffectDef {
    pub name: &'static str,
    pub description: &'static str,
    /// Whether the primary palette colour changes how it looks.
    pub uses_color: bool,
    /// Whether speed / direction matter.
    pub animated: bool,
}

/// The 13 lighting effects exposed by the LK67 firmware.
///
/// Slot 0 is "off"; the rest are the animated/static modes. Firmware effect byte
/// mapping lives in `driver::lk67` so the order here can be reordered freely.
pub const HARDWARE_EFFECTS: [EffectDef; 13] = [
    EffectDef {
        name: "Off",
        description: "All LEDs off.",
        uses_color: false,
        animated: false,
    },
    EffectDef {
        name: "Static",
        description: "Every LED shows one palette colour.",
        uses_color: true,
        animated: false,
    },
    EffectDef {
        name: "Breathing",
        description: "Single colour fades in and out.",
        uses_color: true,
        animated: true,
    },
    EffectDef {
        name: "Rainbow wave",
        description: "Hue sweeps across the board.",
        uses_color: false,
        animated: true,
    },
    EffectDef {
        name: "Rainbow cycle",
        description: "Full spectrum rotates through all keys.",
        uses_color: false,
        animated: true,
    },
    EffectDef {
        name: "Colour wave",
        description: "Palette colours slide across rows.",
        uses_color: true,
        animated: true,
    },
    EffectDef {
        name: "Snake",
        description: "A bright segment runs through the matrix.",
        uses_color: true,
        animated: true,
    },
    EffectDef {
        name: "Reactive ripple",
        description: "Rings expand from keys you press.",
        uses_color: true,
        animated: true,
    },
    EffectDef {
        name: "Reactive sparkle",
        description: "Pressed keys throw off short-lived sparks.",
        uses_color: true,
        animated: true,
    },
    EffectDef {
        name: "Marquee",
        description: "A wide band travels across the keyboard.",
        uses_color: true,
        animated: true,
    },
    EffectDef {
        name: "Twinkle",
        description: "Random keys shimmer and fade.",
        uses_color: true,
        animated: true,
    },
    EffectDef {
        name: "Raindrop",
        description: "Droplets fall down the columns.",
        uses_color: true,
        animated: true,
    },
    EffectDef {
        name: "Fireworks",
        description: "Coloured bursts explode over the keys.",
        uses_color: true,
        animated: true,
    },
];

pub fn effect_def(index: usize) -> &'static EffectDef {
    &HARDWARE_EFFECTS[index.min(HARDWARE_EFFECTS.len() - 1)]
}

/// One painted per-key layout (RGB values are stored unlit; brightness is applied live).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CustomLayer {
    #[serde(default = "default_custom_keys")]
    pub keys: Vec<[u8; 3]>,
}

fn default_custom_keys() -> Vec<[u8; 3]> {
    vec![[0, 0, 0]; KEY_COUNT]
}

impl Default for CustomLayer {
    fn default() -> Self {
        CustomLayer {
            keys: default_custom_keys(),
        }
    }
}

impl CustomLayer {
    pub fn ensure_len(&mut self, len: usize) {
        self.keys.resize(len, [0, 0, 0]);
    }

    pub fn fill(&mut self, color: [u8; 3]) {
        for slot in &mut self.keys {
            *slot = color;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CustomPreset {
    pub name: String,
    #[serde(default)]
    pub keys: Vec<[u8; 3]>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    #[serde(default)]
    pub t_ms: u32,
    #[serde(default)]
    pub keys: Vec<[u8; 3]>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnimPreset {
    pub name: String,
    #[serde(default = "default_fps")]
    pub fps: u8,
    #[serde(default = "default_half")]
    pub speed: u8,
    #[serde(default)]
    pub direction: Direction,
    #[serde(default)]
    pub keyframes: Vec<Keyframe>,
}

fn default_fps() -> u8 {
    30
}

fn default_half() -> u8 {
    50
}

impl Default for AnimPreset {
    fn default() -> Self {
        AnimPreset {
            name: "New animation".into(),
            fps: 30,
            speed: 50,
            direction: Direction::Right,
            keyframes: Vec::new(),
        }
    }
}

impl AnimPreset {
    pub fn ensure_len(&mut self, len: usize) {
        for frame in &mut self.keyframes {
            frame.keys.resize(len, [0, 0, 0]);
        }
        self.keyframes.sort_by_key(|f| f.t_ms);
    }

    pub fn duration_ms(&self) -> u32 {
        self.keyframes.last().map(|f| f.t_ms).unwrap_or(0).max(1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioSource {
    #[default]
    Loopback,
    Microphone,
}

impl AudioSource {
    pub fn label(self) -> &'static str {
        match self {
            AudioSource::Loopback => "System audio (WASAPI loopback)",
            AudioSource::Microphone => "Default microphone",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MusicVisual {
    /// 16 vertical bands spread left to right.
    #[default]
    Equalizer,
    /// Everything pulses with the beat, hue follows level.
    Pulse,
    /// A directional wave driven by bass energy.
    Wave,
    /// Random key twinkles on beats.
    Twinkle,
}

impl MusicVisual {
    pub const ALL: [MusicVisual; 4] = [
        MusicVisual::Equalizer,
        MusicVisual::Pulse,
        MusicVisual::Wave,
        MusicVisual::Twinkle,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MusicVisual::Equalizer => "Equalizer",
            MusicVisual::Pulse => "Pulse",
            MusicVisual::Wave => "Wave",
            MusicVisual::Twinkle => "Twinkle",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MusicConfig {
    #[serde(default)]
    pub source: AudioSource,
    #[serde(default = "default_sensitivity")]
    pub sensitivity: f32,
    /// 0 = jump instantly, 0.95 = very slow.
    #[serde(default = "default_smoothing")]
    pub smoothing: f32,
    #[serde(default = "default_gain")]
    pub gain: f32,
    #[serde(default = "default_color_speed")]
    pub color_speed: f32,
    #[serde(default)]
    pub visual: MusicVisual,
    #[serde(default = "default_beat_boost")]
    pub beat_boost: f32,
    /// Debug aid: show the raw band meter in the UI.
    #[serde(default = "default_true")]
    pub show_meter: bool,
}

fn default_sensitivity() -> f32 {
    1.4
}
fn default_smoothing() -> f32 {
    0.35
}
fn default_gain() -> f32 {
    1.0
}
fn default_color_speed() -> f32 {
    0.5
}
fn default_beat_boost() -> f32 {
    1.0
}

impl Default for MusicConfig {
    fn default() -> Self {
        MusicConfig {
            source: AudioSource::Loopback,
            sensitivity: default_sensitivity(),
            smoothing: default_smoothing(),
            gain: default_gain(),
            color_speed: default_color_speed(),
            visual: MusicVisual::Equalizer,
            beat_boost: default_beat_boost(),
            show_meter: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenRegion {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl ScreenRegion {
    pub fn full() -> Self {
        ScreenRegion {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        }
    }

    pub fn is_full(self) -> bool {
        self.width <= 0 || self.height <= 0
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScreenConfig {
    /// Capture rate, clamped to 5..=30 (the LK67 link cannot stream faster usefully).
    #[serde(default = "default_screen_fps")]
    pub fps: u8,
    #[serde(default = "ScreenRegion::full")]
    pub region: ScreenRegion,
    #[serde(default = "default_screen_smoothing")]
    pub smoothing: f32,
    #[serde(default = "default_screen_saturation")]
    pub saturation: f32,
    #[serde(default = "default_gain")]
    pub gain: f32,
    /// Sampling grid; 16 x 5 lines up one cell per column/row of the keymap.
    #[serde(default = "default_grid_x")]
    pub grid_x: u8,
    #[serde(default = "default_grid_y")]
    pub grid_y: u8,
}

fn default_screen_fps() -> u8 {
    20
}
fn default_screen_smoothing() -> f32 {
    0.45
}
fn default_screen_saturation() -> f32 {
    1.25
}
fn default_grid_x() -> u8 {
    16
}
fn default_grid_y() -> u8 {
    5
}

impl Default for ScreenConfig {
    fn default() -> Self {
        ScreenConfig {
            fps: default_screen_fps(),
            region: ScreenRegion::full(),
            smoothing: default_screen_smoothing(),
            saturation: default_screen_saturation(),
            gain: default_gain(),
            grid_x: default_grid_x(),
            grid_y: default_grid_y(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Lighting {
    #[serde(default)]
    pub mode: LightingMode,
    /// Index into `HARDWARE_EFFECTS`.
    #[serde(default = "default_effect")]
    pub effect: usize,
    #[serde(default = "default_palette")]
    pub colors: [[u8; 3]; PALETTE_SLOTS],
    /// 0..=100, applied on top of effect values.
    #[serde(default = "default_brightness")]
    pub brightness: u8,
    /// 0..=100.
    #[serde(default = "default_half")]
    pub speed: u8,
    #[serde(default)]
    pub direction: Direction,
    #[serde(default)]
    pub custom: CustomLayer,
    #[serde(default)]
    pub animation: AnimPreset,
    #[serde(default)]
    pub animation_playing: bool,
    #[serde(default)]
    pub presets: Vec<CustomPreset>,
    #[serde(default)]
    pub animations: Vec<AnimPreset>,
    #[serde(default)]
    pub music: MusicConfig,
    #[serde(default)]
    pub screen: ScreenConfig,
}

fn default_effect() -> usize {
    1
}
fn default_brightness() -> u8 {
    80
}
fn default_true() -> bool {
    true
}

pub fn default_palette() -> [[u8; 3]; PALETTE_SLOTS] {
    [
        [255, 40, 40],   // red
        [255, 120, 20],  // orange
        [255, 200, 40],  // amber
        [230, 255, 60],  // yellow
        [90, 255, 90],   // lime
        [20, 220, 120],  // green
        [40, 255, 210],  // mint
        [40, 210, 255],  // cyan
        [60, 140, 255],  // sky
        [40, 80, 255],   // blue
        [120, 80, 255],  // indigo
        [190, 80, 255],  // violet
        [255, 60, 200],  // magenta
        [255, 255, 255], // white
    ]
}

impl Default for Lighting {
    fn default() -> Self {
        Lighting::for_slot(0)
    }
}

impl Lighting {
    /// Profiles start with visibly different lighting so switching is obvious.
    pub fn for_slot(slot: usize) -> Self {
        let effect = [1usize, 3, 2, 5][slot % 4];
        let mut lighting = Lighting {
            mode: LightingMode::Hardware,
            effect,
            colors: default_palette(),
            brightness: default_brightness(),
            speed: 50,
            direction: Direction::Right,
            custom: CustomLayer::default(),
            animation: AnimPreset {
                name: "Pulse".into(),
                fps: 30,
                speed: 50,
                direction: Direction::Right,
                keyframes: Vec::new(),
            },
            animation_playing: true,
            presets: Vec::new(),
            animations: Vec::new(),
            music: MusicConfig::default(),
            screen: ScreenConfig::default(),
        };
        let base = lighting.colors[slot % PALETTE_SLOTS];
        lighting
            .custom
            .fill([base[0] / 4, base[1] / 4, base[2] / 4]);
        let dim = [base[0] / 10, base[1] / 10, base[2] / 10];
        lighting.animation.keyframes = vec![
            Keyframe {
                t_ms: 0,
                keys: vec![base; KEY_COUNT],
            },
            Keyframe {
                t_ms: 900,
                keys: vec![dim; KEY_COUNT],
            },
        ];
        lighting
    }

    pub fn effect_def(&self) -> &'static EffectDef {
        effect_def(self.effect)
    }

    pub fn brightness_f32(&self) -> f32 {
        self.brightness as f32 / 100.0
    }

    pub fn ensure_len(&mut self, len: usize) {
        self.custom.ensure_len(len);
        self.animation.ensure_len(len);
        for preset in &mut self.presets {
            preset.keys.resize(len, [0, 0, 0]);
        }
        for anim in &mut self.animations {
            anim.ensure_len(len);
        }
        self.effect = self.effect.min(HARDWARE_EFFECTS.len() - 1);
        self.speed = self.speed.min(100);
        self.brightness = self.brightness.min(100);
        self.music.sensitivity = self.music.sensitivity.clamp(0.1, 5.0);
        self.music.smoothing = self.music.smoothing.clamp(0.0, 0.95);
        self.screen.fps = self.screen.fps.clamp(5, 30);
        self.screen.smoothing = self.screen.smoothing.clamp(0.0, 0.95);
        self.screen.saturation = self.screen.saturation.clamp(0.0, 3.0);
        self.screen.grid_x = self.screen.grid_x.clamp(4, 32);
        self.screen.grid_y = self.screen.grid_y.clamp(2, 16);
    }
}
