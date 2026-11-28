//! Lighting configuration: the 13 hardware effect slots, the 14-colour palette,
//! per-key custom layouts, picture effects, music and screen-follow settings.

use serde::{Deserialize, Serialize};

use crate::model::layout::{KEY_COUNT, Layout};

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
    /// Per-key colours painted here.
    Custom,
    /// Audio reactive.
    Music,
    /// Screen ambilight.
    Screen,
    /// A still picture mapped onto the key positions.
    Picture,
}

impl LightingMode {
    pub const ALL: [LightingMode; 5] = [
        LightingMode::Hardware,
        LightingMode::Custom,
        LightingMode::Picture,
        LightingMode::Music,
        LightingMode::Screen,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LightingMode::Hardware => "Hardware effects",
            LightingMode::Custom => "Custom per-key",
            LightingMode::Music => "Music reactive",
            LightingMode::Screen => "Screen follow",
            LightingMode::Picture => "Picture",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            LightingMode::Hardware => "Hardware",
            LightingMode::Custom => "Custom",
            LightingMode::Music => "Music",
            LightingMode::Screen => "Screen",
            LightingMode::Picture => "Picture",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFit {
    /// Fill the keyboard, cropping the edges of the picture if needed.
    #[default]
    Cover,
    /// Show the whole picture, leaving black bars at the sides or top.
    Contain,
    /// Stretch the picture to the keyboard's bounding box.
    Stretch,
}

impl ImageFit {
    pub const ALL: [ImageFit; 3] = [ImageFit::Cover, ImageFit::Contain, ImageFit::Stretch];

    pub fn label(self) -> &'static str {
        match self {
            ImageFit::Cover => "Cover (crop edges)",
            ImageFit::Contain => "Contain (black bars)",
            ImageFit::Stretch => "Stretch (fill keys)",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PictureConfig {
    /// Path to the last image chosen in the file picker.
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub fit: ImageFit,
}

impl Default for PictureConfig {
    fn default() -> Self {
        PictureConfig {
            path: String::new(),
            fit: ImageFit::Cover,
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

    /// Spread `stops` left to right across the physical key centres, so the
    /// leftmost key starts at the first stop and the right-hand column arrives
    /// at the last one.
    pub fn fill_gradient(&mut self, layout: &Layout, stops: &[[u8; 3]]) {
        if stops.is_empty() {
            return;
        }
        let center = |key: &crate::model::layout::KeyDef| key.x + key.w * 0.5;
        let min = layout.keys.iter().map(center).fold(f32::INFINITY, f32::min);
        let max = layout
            .keys
            .iter()
            .map(center)
            .fold(f32::NEG_INFINITY, f32::max);
        let span = (max - min).max(0.001);
        for key in &layout.keys {
            let t = ((center(key) - min) / span).clamp(0.0, 1.0);
            let scaled = t * (stops.len() - 1) as f32;
            let from = scaled.floor() as usize;
            let to = (from + 1).min(stops.len() - 1);
            let blend = scaled - from as f32;
            let a = stops[from];
            let b = stops[to];
            if let Some(slot) = self.keys.get_mut(key.index) {
                *slot = [
                    (a[0] as f32 + (b[0] as f32 - a[0] as f32) * blend).round() as u8,
                    (a[1] as f32 + (b[1] as f32 - a[1] as f32) * blend).round() as u8,
                    (a[2] as f32 + (b[2] as f32 - a[2] as f32) * blend).round() as u8,
                ];
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CustomPreset {
    pub name: String,
    #[serde(default)]
    pub keys: Vec<[u8; 3]>,
}

fn default_half() -> u8 {
    50
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MusicConfig {
    #[serde(default)]
    pub source: AudioSource,
    #[serde(default = "default_sensitivity")]
    pub sensitivity: f32,
    #[serde(default = "default_gain")]
    pub gain: f32,
    #[serde(default = "default_color_speed")]
    pub color_speed: f32,
    #[serde(default)]
    pub visual: MusicVisual,
    #[serde(default = "default_beat_boost")]
    pub beat_boost: f32,
}

fn default_sensitivity() -> f32 {
    1.4
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
            gain: default_gain(),
            color_speed: default_color_speed(),
            visual: MusicVisual::Equalizer,
            beat_boost: default_beat_boost(),
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
    pub picture: PictureConfig,
    #[serde(default)]
    pub presets: Vec<CustomPreset>,
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
            picture: PictureConfig::default(),
            presets: Vec::new(),
            music: MusicConfig::default(),
            screen: ScreenConfig::default(),
        };
        let base = lighting.colors[slot % PALETTE_SLOTS];
        lighting
            .custom
            .fill([base[0] / 4, base[1] / 4, base[2] / 4]);
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
        for preset in &mut self.presets {
            preset.keys.resize(len, [0, 0, 0]);
        }
        self.effect = self.effect.min(HARDWARE_EFFECTS.len() - 1);
        self.speed = self.speed.min(100);
        self.brightness = self.brightness.min(100);
        self.music.sensitivity = self.music.sensitivity.clamp(0.1, 5.0);
        self.screen.fps = self.screen.fps.clamp(5, 30);
        self.screen.smoothing = self.screen.smoothing.clamp(0.0, 0.95);
        self.screen.saturation = self.screen.saturation.clamp(0.0, 3.0);
        self.screen.grid_x = self.screen.grid_x.clamp(4, 32);
        self.screen.grid_y = self.screen.grid_y.clamp(2, 16);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::layout::layout;

    #[test]
    fn custom_gradient_spans_the_keyboard() {
        let layout = layout();
        let stops = [[255, 0, 0], [0, 255, 0], [255, 255, 255], [255, 0, 255]];
        let mut layer = CustomLayer::default();
        layer.ensure_len(layout.len());
        layer.fill_gradient(&layout, &stops);
        assert_eq!(layer.keys[0], stops[0]);
        let right = layout.index_of_vk(0x27).expect("right arrow");
        assert_eq!(layer.keys[right], stops[3]);
    }
}
