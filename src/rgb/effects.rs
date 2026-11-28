//! The lighting engine: all modes reduced to one colour per key.

use std::f32::consts::TAU;

use crate::driver::lk67::{direction_option, hardware_rgb, uses_direction, wire_mode};
use crate::model::layout::{Layout, ROWS};
use crate::model::lighting::{Direction, Lighting, LightingMode, MusicVisual, PALETTE_SLOTS};
use crate::rgb::music::{AudioSnapshot, BANDS};
use crate::rgb::screen::ScreenSnapshot;

/// Per-key input activity used by reactive effects and the key tester.
#[derive(Clone, Debug, Default)]
pub struct KeyActivity {
    pub pressed: Vec<bool>,
    /// 1.0 at press time, decays over ~0.5 s. Drives ripple rings.
    pub pulse: Vec<f32>,
    /// Seconds since the key was last pressed.
    pub age: Vec<f32>,
}

impl KeyActivity {
    pub fn new(len: usize) -> Self {
        KeyActivity {
            pressed: vec![false; len],
            pulse: vec![0.0; len],
            age: vec![f32::MAX; len],
        }
    }

    pub fn press(&mut self, index: usize) {
        if let Some(slot) = self.pressed.get_mut(index) {
            *slot = true;
        }
        if let Some(slot) = self.pulse.get_mut(index) {
            *slot = 1.0;
        }
        if let Some(slot) = self.age.get_mut(index) {
            *slot = 0.0;
        }
    }

    pub fn release(&mut self, index: usize) {
        if let Some(slot) = self.pressed.get_mut(index) {
            *slot = false;
        }
    }

    pub fn tick(&mut self, dt: f32) {
        let decay = (-dt * 2.4).exp();
        for pulse in &mut self.pulse {
            *pulse *= decay;
            if *pulse < 0.002 {
                *pulse = 0.0;
            }
        }
        for age in &mut self.age {
            if *age < 3600.0 {
                *age += dt;
            }
        }
    }

    pub fn any_pressed(&self) -> bool {
        self.pressed.iter().any(|p| *p)
    }
}

pub struct RgbEngine {
    pub colors: Vec<[u8; 3]>,
    time: f32,
    beat: f32,
    sparks: Vec<f32>,
    spark_hue: Vec<f32>,
    /// Real-time frames since the hardware preview started (yc500 ticks at 5 ms).
    hardware_frame: f64,
    /// Wire mode + app effect the frame counter belongs to, so a new effect restarts at phase 0.
    hardware_key: Option<(u8, usize)>,
    screen_smooth: Vec<[f32; 3]>,
    music_smooth: [f32; BANDS],
    music_level: f32,
    rng: u64,
}

pub struct LiveInputs<'a> {
    pub audio: &'a AudioSnapshot,
    pub screen: &'a ScreenSnapshot,
}

impl Default for RgbEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RgbEngine {
    pub fn new() -> Self {
        RgbEngine {
            colors: Vec::new(),
            time: 0.0,
            beat: 0.0,
            sparks: Vec::new(),
            spark_hue: Vec::new(),
            hardware_frame: 0.0,
            hardware_key: None,
            screen_smooth: Vec::new(),
            music_smooth: [0.0; BANDS],
            music_level: 0.0,
            rng: 0x9E37_79B9_7F4A_7C15,
        }
    }

    fn ensure_len(&mut self, len: usize) {
        self.colors.resize(len, [0, 0, 0]);
        self.sparks.resize(len, 0.0);
        self.spark_hue.resize(len, 0.0);
        self.screen_smooth.resize(len, [0.0, 0.0, 0.0]);
    }

    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        (x >> 40) as f32 / 16_777_216.0
    }

    /// Compute one frame of lighting. `dt` is clamped internally to keep effects sane
    /// after the window was minimised or the app was paused.
    pub fn render(
        &mut self,
        lighting: &Lighting,
        layout: &Layout,
        dt: f32,
        live: &LiveInputs<'_>,
        picture: Option<&[[u8; 3]]>,
    ) -> &[[u8; 3]] {
        let n = layout.len();
        self.ensure_len(n);
        let dt = dt.clamp(0.0, 0.1);
        self.time += dt;
        self.beat = (self.beat - dt * 3.2).max(0.0).max(live.audio.beat);
        if self.beat > 1.0 {
            self.beat = 1.0;
        }

        self.decay_sparks(dt);
        if !matches!(lighting.mode, LightingMode::Hardware) {
            self.hardware_key = None;
        }
        match lighting.mode {
            LightingMode::Hardware => self.hardware(lighting, layout, dt, live),
            LightingMode::Custom => self.custom(lighting, layout),
            LightingMode::Picture => self.picture(lighting, layout, picture),
            LightingMode::Music => self.music(lighting, layout, dt, live),
            LightingMode::Screen => self.screen(lighting, layout, dt, live),
        }

        let master = if matches!(lighting.mode, LightingMode::Hardware) {
            ((lighting.brightness.min(100) as u16 * 4) / 100) as f32 / 4.0
        } else {
            lighting.brightness_f32()
        };
        for color in &mut self.colors {
            *color = scale(*color, master);
        }
        &self.colors
    }

    fn decay_sparks(&mut self, dt: f32) {
        for spark in &mut self.sparks {
            *spark = (*spark - dt * 1.7).max(0.0);
        }
    }

    fn hardware(&mut self, lighting: &Lighting, layout: &Layout, dt: f32, _live: &LiveInputs<'_>) {
        // The yc500 firmware steps its effects on a 5 ms tick. Drive the
        // preview from the same counter so phase and speed line up.
        let mode = wire_mode(lighting);
        let key = (mode, lighting.effect);
        if self.hardware_key != Some(key) {
            self.hardware_frame = 0.0;
            self.hardware_key = Some(key);
        }
        self.hardware_frame += dt as f64 / 0.005;
        let rainbow = matches!(lighting.effect, 3 | 4);
        let option = if uses_direction(lighting.effect) {
            direction_option(lighting.direction)
        } else {
            0
        };
        let rgb = hardware_rgb(lighting);
        let speed = ((lighting.speed.min(100) as u16 * 4) / 100) as u8;
        for (i, key) in layout.keys.iter().enumerate() {
            self.colors[i] = yc500_color(
                mode,
                rainbow,
                rgb,
                option,
                speed,
                key.matrix as usize,
                i,
                self.hardware_frame,
            );
        }
    }

    fn custom(&mut self, lighting: &Lighting, layout: &Layout) {
        for i in 0..layout.len() {
            self.colors[i] = *lighting.custom.keys.get(i).unwrap_or(&[0, 0, 0]);
        }
    }

    fn picture(&mut self, lighting: &Lighting, layout: &Layout, picture: Option<&[[u8; 3]]>) {
        for i in 0..layout.len() {
            self.colors[i] = *picture
                .and_then(|keys| keys.get(i))
                .or_else(|| lighting.custom.keys.get(i))
                .unwrap_or(&[0, 0, 0]);
        }
    }

    fn music(&mut self, lighting: &Lighting, layout: &Layout, dt: f32, live: &LiveInputs<'_>) {
        let cfg = &lighting.music;
        // Fixed preview smoothing; the keyboard's bars use the raw band stream.
        let follow = 0.65 * (dt * 60.0).clamp(0.15, 2.0);
        let mut max_band = 0.0f32;
        for band in 0..BANDS {
            let target = (live.audio.bands[band] * cfg.sensitivity * cfg.gain).clamp(0.0, 1.5);
            self.music_smooth[band] += (target - self.music_smooth[band]) * follow;
            max_band = max_band.max(self.music_smooth[band]);
        }
        self.music_level +=
            ((live.audio.level * cfg.sensitivity).clamp(0.0, 1.5) - self.music_level) * follow;
        let level = self.music_level.max(max_band * 0.6);
        let hue_shift = (self.time * cfg.color_speed * 0.25).rem_euclid(1.0);
        let primary = lighting.colors[0];

        match cfg.visual {
            MusicVisual::Equalizer => {
                for (i, key) in layout.keys.iter().enumerate() {
                    let band =
                        ((center_x(key, layout) * (BANDS as f32 - 0.01)) as usize).min(BANDS - 1);
                    let value = (self.music_smooth[band] + self.beat * 0.25 * cfg.beat_boost)
                        .clamp(0.0, 1.4);
                    let color = lighting.colors[band % PALETTE_SLOTS];
                    let base = scale(color, 0.05 + value.powf(0.8));
                    let tint = hsv(
                        (hue_shift + band as f32 / BANDS as f32).rem_euclid(1.0),
                        0.5,
                        1.0,
                    );
                    self.colors[i] = mix(base, tint, 0.15 * value);
                }
            }
            MusicVisual::Pulse => {
                let value = (level + self.beat * 0.5 * cfg.beat_boost).clamp(0.0, 1.4);
                let color = mix(primary, hsv(hue_shift, 0.85, 1.0), 0.45);
                self.colors.fill(scale(color, 0.06 + value));
            }
            MusicVisual::Wave => {
                for (i, key) in layout.keys.iter().enumerate() {
                    let phase = dir_phase(lighting.direction, center_x(key, layout), center_y(key));
                    let wave = ((phase * 2.5 - self.time * 2.0) * TAU).sin() * 0.5 + 0.5;
                    let value = (level * 0.9 + self.beat * 0.4 * cfg.beat_boost) * (0.35 + wave);
                    let color = mix(
                        primary,
                        hsv((hue_shift + phase).rem_euclid(1.0), 0.9, 1.0),
                        0.7,
                    );
                    self.colors[i] = scale(color, 0.04 + value.clamp(0.0, 1.3));
                }
            }
            MusicVisual::Twinkle => {
                self.colors.fill(scale(primary, 0.03 + level * 0.1));
                if live.audio.beat > 0.85 {
                    let key_count = 3 + (self.beat * 6.0) as usize;
                    for _ in 0..key_count {
                        let index = (self.rand() * layout.len() as f32) as usize;
                        let index = index.min(layout.len() - 1);
                        self.sparks[index] = 1.0;
                        self.spark_hue[index] = self.rand();
                    }
                }
                for i in 0..layout.len() {
                    if self.sparks[i] > 0.0 {
                        let color = hsv(self.spark_hue[i], 0.85, 1.0);
                        self.colors[i] = add_sat(self.colors[i], scale(color, self.sparks[i]));
                    }
                }
            }
        }
    }

    fn screen(&mut self, lighting: &Lighting, layout: &Layout, dt: f32, live: &LiveInputs<'_>) {
        let cfg = &lighting.screen;
        let snap = live.screen;
        if !snap.active || snap.grid.is_empty() {
            // Show a clear "no signal" state instead of a black keyboard.
            let pulse = (self.time * 1.4).sin() * 0.5 + 0.5;
            for i in 0..layout.len() {
                self.colors[i] = scale([20, 30, 45], 0.4 + pulse * 0.4);
            }
            return;
        }
        let follow = (1.0 - cfg.smoothing).clamp(0.02, 1.0) * (dt * 60.0).clamp(0.2, 2.0);
        for (i, key) in layout.keys.iter().enumerate() {
            let gw = snap.gw.max(1);
            let gh = snap.gh.max(1);
            let gx = ((center_x(key, layout) * gw as f32) as usize).min(gw - 1);
            let gy = ((center_y(key) * gh as f32) as usize).min(gh - 1);
            let cell = snap.grid.get(gy * gw + gx).copied().unwrap_or([0, 0, 0]);
            let target = [
                (cell[0] as f32 / 255.0 * cfg.gain).clamp(0.0, 1.0),
                (cell[1] as f32 / 255.0 * cfg.gain).clamp(0.0, 1.0),
                (cell[2] as f32 / 255.0 * cfg.gain).clamp(0.0, 1.0),
            ];
            let smooth = &mut self.screen_smooth[i];
            for c in 0..3 {
                smooth[c] += (target[c] - smooth[c]) * follow;
            }
            let saturated = saturate(
                [smooth[0], smooth[1], smooth[2]],
                cfg.saturation.clamp(0.0, 3.0),
            );
            self.colors[i] = [
                (saturated[0].clamp(0.0, 1.0) * 255.0) as u8,
                (saturated[1].clamp(0.0, 1.0) * 255.0) as u8,
                (saturated[2].clamp(0.0, 1.0) * 255.0) as u8,
            ];
        }
    }
}

/// LED grid used by the yc500 effect engine on the LK67: 6 rows x 15 columns.
const YC500_ROWS: usize = 6;
const YC500_COLS: usize = 15;
const HUE_CYCLE: f64 = 6.0 * 255.0;

/// The firmware's seven-colour preset palette.
const PRESETS: [[u8; 3]; 7] = [
    [255, 0, 0],
    [255, 128, 0],
    [255, 255, 0],
    [0, 255, 0],
    [0, 255, 255],
    [0, 0, 255],
    [255, 0, 255],
];

/// yc500 speed tables, indexed by the wire byte (1..=5).
const BREATH: [u16; 6] = [10, 40, 60, 80, 100, 200];
const BREATH_HOLD: u16 = 20;
const SPECTRUM: [u16; 6] = [5, 4, 3, 2, 1, 0];
const HUE: [u16; 6] = [5, 4, 3, 2, 1, 0];
const RIPPLE: [u16; 6] = [5, 7, 9, 11, 13, 12];
const RADIANT: [u16; 6] = [5, 7, 9, 11, 13, 12];
const STARS: [u16; 6] = [5, 10, 15, 20, 25, 12];
const FLOW: [u16; 6] = [5, 10, 15, 20, 25, 12];
const LAYERS_SPEED: [u16; 6] = [2, 3, 5, 7, 9, 11];
const SINE: [u16; 6] = [5, 7, 10, 14, 17, 20];

const LAYERS_COUNTS: &[u16] = &[
    4, 4, 6, 6, 5, 5, 5, 6, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 255, 13, 11, 11, 10, 8, 11,
    12, 8, 10, 0,
];
const LAYERS_CELLS: &[(u8, u8)] = &[
    (0, 0),
    (1, 0),
    (5, 17),
    (5, 16),
    (2, 0),
    (1, 1),
    (4, 16),
    (3, 16),
    (0, 2),
    (1, 2),
    (2, 1),
    (5, 15),
    (4, 15),
    (3, 15),
    (0, 3),
    (1, 3),
    (2, 2),
    (5, 14),
    (4, 14),
    (3, 14),
    (0, 4),
    (1, 4),
    (2, 3),
    (5, 13),
    (4, 13),
    (0, 5),
    (1, 5),
    (2, 4),
    (5, 12),
    (3, 13),
    (0, 6),
    (1, 6),
    (2, 5),
    (5, 11),
    (4, 12),
    (0, 7),
    (1, 7),
    (2, 6),
    (5, 10),
    (4, 11),
    (3, 11),
    (1, 10),
    (2, 10),
    (3, 10),
    (4, 10),
    (4, 9),
    (4, 8),
    (3, 7),
    (2, 7),
    (1, 8),
    (1, 9),
    (2, 9),
    (3, 9),
    (3, 8),
    (2, 8),
    (1, 10),
    (2, 10),
    (3, 10),
    (4, 10),
    (4, 9),
    (4, 8),
    (3, 7),
    (2, 7),
    (1, 8),
    (1, 9),
    (2, 9),
    (3, 9),
    (3, 8),
    (2, 8),
    (255, 255),
    (1, 10),
    (2, 10),
    (3, 10),
    (4, 10),
    (4, 9),
    (4, 8),
    (3, 7),
    (2, 7),
    (1, 8),
    (1, 9),
    (0, 8),
    (0, 9),
    (5, 9),
    (0, 10),
    (1, 11),
    (2, 11),
    (3, 11),
    (4, 11),
    (5, 10),
    (0, 7),
    (1, 7),
    (2, 6),
    (3, 6),
    (4, 7),
    (0, 11),
    (1, 12),
    (2, 12),
    (4, 12),
    (5, 11),
    (0, 6),
    (1, 6),
    (2, 5),
    (3, 5),
    (4, 6),
    (5, 6),
    (0, 12),
    (1, 13),
    (2, 13),
    (3, 13),
    (5, 12),
    (0, 5),
    (1, 5),
    (2, 4),
    (3, 4),
    (4, 5),
    (0, 13),
    (4, 13),
    (5, 13),
    (0, 4),
    (1, 4),
    (2, 3),
    (3, 3),
    (4, 4),
    (0, 14),
    (1, 14),
    (2, 14),
    (3, 14),
    (4, 14),
    (5, 14),
    (0, 3),
    (1, 3),
    (2, 2),
    (3, 2),
    (4, 3),
    (0, 15),
    (1, 15),
    (2, 15),
    (3, 15),
    (4, 15),
    (5, 15),
    (0, 2),
    (1, 2),
    (2, 1),
    (3, 1),
    (4, 2),
    (5, 3),
    (0, 16),
    (1, 16),
    (2, 16),
    (3, 16),
    (4, 16),
    (5, 16),
    (1, 1),
    (5, 2),
    (0, 17),
    (1, 17),
    (3, 17),
    (5, 17),
    (0, 0),
    (1, 0),
    (2, 0),
    (3, 0),
    (4, 0),
    (5, 0),
];
const LAYERS_PAUSE: u64 = 5;
const LAYERS_REST: u64 = 25;
const LAYERS_SCRIPT_COLS: usize = 18;

fn yc500_step_frames(mode: u8, wire: u8) -> f64 {
    let i = wire.min(5) as usize;
    match mode {
        5 => RIPPLE[i] as f64,
        14 => RADIANT[i] as f64,
        6 => STARS[i] as f64,
        7 => FLOW[i] as f64,
        9 => LAYERS_SPEED[i] as f64,
        10 => SINE[i] as f64,
        _ => 8.0,
    }
}

fn yc500_hue_turns(mode: u8, wire: u8, frame: f64) -> f64 {
    let i = wire.min(5) as usize;
    if mode == 3 {
        (frame / 2.0).floor() * SPECTRUM[i] as f64 / HUE_CYCLE
    } else {
        frame * (HUE[i] as f64 + 1.0) / HUE_CYCLE
    }
}

fn yc500_breath(wire: u8, frame: f64) -> (f32, i64) {
    let ramp = BREATH[wire.min(5) as usize] as f64;
    let total = 2.0 * (ramp + BREATH_HOLD as f64);
    let cycle = (frame / total).floor() as i64;
    let f = frame.rem_euclid(total);
    let level = if f < ramp {
        f / ramp
    } else if f < ramp + BREATH_HOLD as f64 {
        1.0
    } else if f < 2.0 * ramp + BREATH_HOLD as f64 {
        1.0 - (f - ramp - BREATH_HOLD as f64) / ramp
    } else {
        0.0
    };
    (level as f32, cycle)
}

fn yc500_base(rainbow: bool, rgb: [u8; 3], step: i64) -> [u8; 3] {
    if rainbow {
        PRESETS[step.rem_euclid(7) as usize]
    } else {
        rgb
    }
}

fn glow(level: f32) -> f32 {
    level.clamp(0.0, 1.0).powf(0.6)
}

fn noise(i: i64, n: i64) -> f32 {
    let mut h = (i
        .wrapping_mul(374_761_393)
        .wrapping_add(n.wrapping_mul(668_265_263))) as i32;
    h = (h ^ ((h as u32) >> 13) as i32).wrapping_mul(1_274_126_177);
    ((h ^ ((h as u32) >> 16) as i32) as u32) as f32 / 4_294_967_296.0
}

fn yc500_layers_total() -> u64 {
    let mut step = 0u64;
    for &n in LAYERS_COUNTS {
        if n == 0 {
            break;
        }
        if n == 255 {
            step += LAYERS_PAUSE;
        } else {
            step += 1;
        }
    }
    step
}

fn yc500_layers_lit(row: usize, col: usize, now: u64) -> Option<usize> {
    let mut step = 0u64;
    let mut phase = 0usize;
    let mut at = 0usize;
    let mut lit = None;
    for &n in LAYERS_COUNTS {
        if n == 0 {
            break;
        }
        if n == 255 {
            step += LAYERS_PAUSE;
            phase += 1;
            at += 1;
            continue;
        }
        for _ in 0..n {
            if let Some(&(r, c)) = LAYERS_CELLS.get(at) {
                if r as usize == row && c as usize == col && step <= now {
                    lit = Some(phase);
                }
            }
            at += 1;
        }
        step += 1;
    }
    lit
}

#[allow(clippy::too_many_arguments)]
fn yc500_color(
    mode: u8,
    rainbow: bool,
    rgb: [u8; 3],
    option: u8,
    speed_level: u8,
    slot: usize,
    index: usize,
    frame: f64,
) -> [u8; 3] {
    let wire = 5u8.saturating_sub(speed_level).clamp(1, 5);
    let step_frames = yc500_step_frames(mode, wire);
    let step = (frame / step_frames).floor();
    let step_phase = (frame.rem_euclid(step_frames)) / step_frames;
    let turns = yc500_hue_turns(mode, wire, frame);
    let row = slot % YC500_ROWS;
    let col = (slot / YC500_ROWS).min(YC500_COLS - 1);
    let x = col as f32 / (YC500_COLS - 1) as f32;
    let y = row as f32 / (YC500_ROWS - 1) as f32;

    match mode {
        1 | 19 => {
            if rainbow {
                hsv(x, 1.0, 1.0)
            } else {
                rgb
            }
        }
        2 => {
            let (level, cycle) = yc500_breath(wire, frame);
            scale(yc500_base(rainbow, rgb, cycle), glow(level))
        }
        3 => hsv(turns as f32, 1.0, 1.0),
        4 => {
            let along = wave_along(option, x, y);
            if rainbow {
                hsv(along + turns as f32, 1.0, 1.0)
            } else {
                let wave =
                    0.5 + 0.5 * (((along as f64 + turns) * std::f64::consts::TAU).cos() as f32);
                scale(rgb, glow(wave))
            }
        }
        5 => {
            let life = 24.0;
            let burst = (step / life).floor();
            let ox = noise(1, burst as i64);
            let oy = noise(2, burst as i64);
            let dx = (x - ox) * YC500_COLS as f32;
            let dy = (y - oy) * YC500_ROWS as f32;
            let d = (dx * dx + dy * dy).sqrt();
            let radius = (step % life) as f32 + step_phase as f32;
            scale(
                yc500_base(rainbow, rgb, burst as i64),
                glow((1.0 - (d - radius).abs() * 0.8).max(0.0)),
            )
        }
        6 => {
            let life: f64 = 40.0;
            let born = step - ((noise(index as i64, 0) as f64 * life).floor() % life);
            let age = ((step - born) % life + life) % life;
            let level = if age < 6.0 {
                age / 6.0
            } else {
                (1.0 - (age - 6.0) / (life - 6.0)).max(0.0)
            };
            let tone = if rainbow {
                hsv(noise(index as i64, (step / life).floor() as i64), 1.0, 1.0)
            } else {
                rgb
            };
            scale(tone, glow(level as f32))
        }
        7 => {
            let cells = YC500_ROWS * YC500_COLS;
            let pos = if option == 1 {
                let ring = row
                    .min(YC500_ROWS - 1 - row)
                    .min(col)
                    .min(YC500_COLS - 1 - col);
                ring * 40
                    + if row <= YC500_ROWS / 2 {
                        col
                    } else {
                        YC500_COLS * 2 - col
                    }
            } else {
                row * YC500_COLS
                    + if row % 2 == 1 {
                        YC500_COLS - 1 - col
                    } else {
                        col
                    }
            };
            let head = step % cells as f64 + step_phase;
            let dist = ((pos as f64 - head) % cells as f64 + cells as f64) % cells as f64;
            let tail = 6.0;
            let level = if dist > cells as f64 - tail {
                (dist - (cells as f64 - tail)) / tail
            } else {
                0.0
            };
            scale(
                yc500_base(rainbow, rgb, (step / cells as f64).floor() as i64),
                glow(level as f32),
            )
        }
        8 => {
            let hit = (noise(3, step as i64) * 100.0).floor();
            if (index % 100) as f32 == hit {
                scale(
                    yc500_base(rainbow, rgb, step as i64),
                    glow(1.0 - step_phase as f32),
                )
            } else {
                [0, 0, 0]
            }
        }
        9 => {
            let total = yc500_layers_total() + LAYERS_REST;
            let cycle = (step as u64) / total;
            let now = (step as u64) % total;
            let script_col = ((col * LAYERS_SCRIPT_COLS) / YC500_COLS).min(LAYERS_SCRIPT_COLS - 1);
            match yc500_layers_lit(row, script_col, now) {
                Some(phase) => {
                    if rainbow {
                        PRESETS[((cycle * 2 + phase as u64) % 6) as usize]
                    } else {
                        rgb
                    }
                }
                None => [0, 0, 0],
            }
        }
        10 => {
            let center = (YC500_COLS / 2) as i32;
            let mut level = 0.0;
            for age in 0..20 {
                let born = step - age as f64;
                if born < 0.0 {
                    break;
                }
                let r = (2.5 + 2.5 * ((born / 30.0) * std::f64::consts::TAU).sin()).round() as i32;
                if r != row as i32 {
                    continue;
                }
                if col as i32 == center - age as i32 || col as i32 == center + age as i32 {
                    level = 1.0;
                }
            }
            scale(
                yc500_base(rainbow, rgb, (step / 30.0).floor() as i64),
                glow(level),
            )
        }
        14 => {
            let a = ((y - 0.5) * 0.4).atan2(x - 0.5) / std::f32::consts::TAU;
            hsv(a + ((step + step_phase) / 24.0) as f32, 1.0, 1.0)
        }
        17 => {
            let drop = frame / (12.0 * YC500_ROWS as f64) + noise(col as i64, 0) as f64;
            let fall = drop.rem_euclid(1.0);
            let d = (y as f64 - fall).abs();
            let flake = if rainbow {
                PRESETS[((noise(col as i64, drop.floor() as i64 + 1) * 7.0).floor() as usize) % 7]
            } else {
                rgb
            };
            let visible = if noise(col as i64, 1) > 0.4 { 1.0 } else { 0.3 };
            scale(flake, glow(((1.0 - d * 5.0).max(0.0) * visible) as f32))
        }
        18 => {
            let drop = frame / (8.0 * YC500_ROWS as f64) + noise(col as i64, 0) as f64;
            let head = drop.rem_euclid(1.5);
            let d = head - y as f64;
            let active = noise(col as i64, (drop / 1.5).floor() as i64) > 0.6;
            let level = if active && d >= 0.0 && d < 0.5 {
                1.0 - d * 2.0
            } else {
                0.0
            };
            scale(yc500_base(rainbow, rgb, col as i64), glow(level as f32))
        }
        _ => rgb,
    }
}

/// Position along the travelling wave for yc500 mode 4.
///
/// The hue is `along + time`, so a feature moves toward larger `along`. The
/// firmware counts its columns from the left and option 0 is "left to right",
/// which means `along` has to fall as `x` grows for that option.
fn wave_along(option: u8, x: f32, y: f32) -> f32 {
    match option % 4 {
        0 => 1.0 - x,
        1 => x,
        2 => y,
        _ => 1.0 - y,
    }
}

fn center_x(key: &crate::model::layout::KeyDef, layout: &Layout) -> f32 {
    ((key.x + key.w * 0.5) / layout.width).clamp(0.0, 1.0)
}

fn center_y(key: &crate::model::layout::KeyDef) -> f32 {
    ((key.y + key.h * 0.5) / ROWS as f32).clamp(0.0, 1.0)
}

pub fn dir_phase(direction: Direction, nx: f32, ny: f32) -> f32 {
    match direction {
        Direction::Right => nx,
        Direction::Left => 1.0 - nx,
        Direction::Up => 1.0 - ny,
        Direction::Down => ny,
        Direction::Clockwise => (ny - 0.5).atan2(nx - 0.5) / TAU + 0.5,
        Direction::CounterClockwise => (-(ny - 0.5)).atan2(nx - 0.5) / TAU + 0.5,
    }
}

pub fn hsv(h: f32, s: f32, v: f32) -> [u8; 3] {
    let h = h.rem_euclid(1.0) * 6.0;
    let i = h.floor() as i32;
    let f = h - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match i.rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [
        (r.clamp(0.0, 1.0) * 255.0) as u8,
        (g.clamp(0.0, 1.0) * 255.0) as u8,
        (b.clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

pub fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        (a[0] as f32 + (b[0] as f32 - a[0] as f32) * t) as u8,
        (a[1] as f32 + (b[1] as f32 - a[1] as f32) * t) as u8,
        (a[2] as f32 + (b[2] as f32 - a[2] as f32) * t) as u8,
    ]
}

pub fn scale(color: [u8; 3], factor: f32) -> [u8; 3] {
    let f = factor.clamp(0.0, 2.0);
    [
        (color[0] as f32 * f).min(255.0) as u8,
        (color[1] as f32 * f).min(255.0) as u8,
        (color[2] as f32 * f).min(255.0) as u8,
    ]
}

fn add_sat(a: [u8; 3], b: [u8; 3]) -> [u8; 3] {
    [
        a[0].saturating_add(b[0]),
        a[1].saturating_add(b[1]),
        a[2].saturating_add(b[2]),
    ]
}

fn saturate(rgb: [f32; 3], amount: f32) -> [f32; 3] {
    let luma = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
    [
        (luma + (rgb[0] - luma) * amount).clamp(0.0, 1.0),
        (luma + (rgb[1] - luma) * amount).clamp(0.0, 1.0),
        (luma + (rgb[2] - luma) * amount).clamp(0.0, 1.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::layout::layout;
    use crate::rgb::screen::ScreenSnapshot;

    fn render_frames(effect: usize, frames: usize) -> (Vec<[u8; 3]>, bool) {
        let layout = layout();
        let mut lighting = Lighting::default();
        lighting.mode = LightingMode::Hardware;
        lighting.effect = effect;
        lighting.brightness = 100;
        let mut engine = RgbEngine::new();
        let audio = AudioSnapshot::default();
        let screen = ScreenSnapshot::default();
        let live = LiveInputs {
            audio: &audio,
            screen: &screen,
        };
        let mut last = vec![[0u8; 3]; layout.len()];
        let mut bright = false;
        for _ in 0..frames {
            last = engine
                .render(&lighting, layout, 0.016, &live, None)
                .to_vec();
            bright |= last.iter().any(|c| *c != [0, 0, 0]);
        }
        (last, bright)
    }

    #[test]
    fn every_effect_produces_a_frame() {
        for effect in 0..13 {
            let (last, bright) = render_frames(effect, 90);
            assert_eq!(last.len(), 68, "effect {effect}");
            if effect > 0 {
                assert!(bright, "effect {effect} is completely dark");
            } else {
                assert!(last.iter().all(|c| *c == [0, 0, 0]), "effect 0 must be off");
            }
        }
    }

    fn wave_peak_column(option: u8, frame: f64) -> usize {
        let mut best = (0usize, -1.0f32);
        for col in 0..15usize {
            let slot = col * 6 + 1;
            let color = yc500_color(4, false, [255, 255, 255], option, 2, slot, 0, frame);
            let level = color[0] as f32 + color[1] as f32 + color[2] as f32;
            if level > best.1 {
                best = (col, level);
            }
        }
        best.0
    }

    #[test]
    fn wave_travels_left_to_right_for_option_zero() {
        // Speed level 2 is the default 50: three hue steps per 5 ms frame, so
        // frames 102 and 153 place the crest at x = 0.2 and x = 0.3.
        assert!(wave_peak_column(0, 153.0) > wave_peak_column(0, 102.0));
        assert!(wave_peak_column(1, 153.0) < wave_peak_column(1, 102.0));
    }

    #[test]
    fn wave_direction_matches_its_label() {
        // Option 0 is "left to right": along must decrease with x so the
        // advancing hue carries the wave rightwards. Option 1 reverses it.
        assert!(wave_along(0, 0.9, 0.5) < wave_along(0, 0.1, 0.5));
        assert!(wave_along(1, 0.9, 0.5) > wave_along(1, 0.1, 0.5));
        // Verticals keep their previous mapping.
        assert!(wave_along(2, 0.5, 0.9) > wave_along(2, 0.5, 0.1));
        assert!(wave_along(3, 0.5, 0.9) < wave_along(3, 0.5, 0.1));
    }

    #[test]
    fn music_and_screen_modes_are_safe_without_input() {
        let layout = layout();
        for mode in [
            LightingMode::Music,
            LightingMode::Screen,
            LightingMode::Custom,
            LightingMode::Picture,
        ] {
            let mut lighting = Lighting::default();
            lighting.mode = mode;
            let mut engine = RgbEngine::new();
            let audio = AudioSnapshot::default();
            let screen = ScreenSnapshot::default();
            let live = LiveInputs {
                audio: &audio,
                screen: &screen,
            };
            let colors = engine.render(&lighting, layout, 0.016, &live, None);
            assert_eq!(colors.len(), 68);
        }
    }
}
