//! The lighting engine: all modes reduced to one colour per key.

use std::f32::consts::TAU;

use crate::model::layout::{Layout, ROWS};
use crate::model::lighting::{
    AnimPreset, Direction, Lighting, LightingMode, MusicVisual, PALETTE_SLOTS,
};
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

#[derive(Clone, Copy, Debug)]
struct Burst {
    x: f32,
    y: f32,
    hue: f32,
    age: f32,
    life: f32,
}

pub struct RgbEngine {
    pub colors: Vec<[u8; 3]>,
    time: f32,
    beat: f32,
    sparks: Vec<f32>,
    spark_hue: Vec<f32>,
    bursts: Vec<Burst>,
    burst_timer: f32,
    anim_time: f32,
    anim_accum: f32,
    anim_colors: Vec<[u8; 3]>,
    screen_smooth: Vec<[f32; 3]>,
    music_smooth: [f32; BANDS],
    music_level: f32,
    rng: u64,
}

pub struct LiveInputs<'a> {
    pub activity: &'a KeyActivity,
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
            bursts: Vec::new(),
            burst_timer: 0.0,
            anim_time: 0.0,
            anim_accum: 0.0,
            anim_colors: Vec::new(),
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
        self.anim_colors.resize(len, [0, 0, 0]);
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
        match lighting.mode {
            LightingMode::Hardware => self.hardware(lighting, layout, dt, live),
            LightingMode::Custom => self.custom(lighting, layout, dt),
            LightingMode::Music => self.music(lighting, layout, dt, live),
            LightingMode::Screen => self.screen(lighting, layout, dt, live),
        }

        let master = lighting.brightness_f32();
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

    fn hardware(&mut self, lighting: &Lighting, layout: &Layout, dt: f32, live: &LiveInputs<'_>) {
        let speed = 0.2 + (lighting.speed as f32 / 100.0) * 3.0;
        let direction = lighting.direction;
        let primary = lighting.colors[0];
        match lighting.effect {
            0 => {
                self.colors.fill([0, 0, 0]);
            }
            1 => {
                self.colors.fill(primary);
            }
            2 => {
                let wave = (self.time * speed * 0.7).sin() * 0.5 + 0.5;
                let value = 0.08 + 0.92 * wave.powf(1.6);
                self.colors.fill(scale(primary, value));
            }
            3 => {
                for (i, key) in layout.keys.iter().enumerate() {
                    let nx = center_x(key, layout);
                    let ny = center_y(key);
                    let phase = dir_phase(direction, nx, ny);
                    let hue = (phase + self.time * speed * 0.12).rem_euclid(1.0);
                    let wave = 0.55 + 0.45 * ((phase * 6.0 - self.time * speed).sin() * 0.5 + 0.5);
                    self.colors[i] = scale(hsv(hue, 1.0, 1.0), wave);
                }
            }
            4 => {
                for i in 0..layout.len() {
                    let hue = (self.time * speed * 0.08 + i as f32 * 0.012).rem_euclid(1.0);
                    self.colors[i] = hsv(hue, 1.0, 1.0);
                }
            }
            5 => {
                for (i, key) in layout.keys.iter().enumerate() {
                    let nx = center_x(key, layout);
                    let phase = dir_phase(direction, nx, center_y(key));
                    let pos = (phase * PALETTE_SLOTS as f32 + self.time * speed * 2.2)
                        .rem_euclid(PALETTE_SLOTS as f32);
                    let a = pos.floor() as usize % PALETTE_SLOTS;
                    let b = (a + 1) % PALETTE_SLOTS;
                    let t = pos.fract();
                    let color = mix(lighting.colors[a], lighting.colors[b], t);
                    let wave =
                        0.45 + 0.55 * ((phase * 4.0 - self.time * speed * 1.5).sin() * 0.5 + 0.5);
                    self.colors[i] = scale(color, wave);
                }
            }
            6 => {
                let len = layout.len() as f32;
                let pos = (self.time * speed * 0.35).rem_euclid(1.0) * len;
                for i in 0..layout.len() {
                    let index = match direction {
                        Direction::Left | Direction::Up | Direction::CounterClockwise => {
                            (layout.len() - 1 - i) as f32
                        }
                        _ => i as f32,
                    };
                    let tail = 10.0;
                    let mut d = (pos - index).rem_euclid(len);
                    if d > len - tail {
                        d = len - d;
                    }
                    let value = if d < tail {
                        (1.0 - d / tail).powf(1.8)
                    } else {
                        0.0
                    };
                    self.colors[i] = scale(primary, value.max(0.03));
                }
            }
            7 => {
                for i in 0..layout.len() {
                    self.colors[i] = [0, 0, 0];
                }
                for (source, pulse) in live.activity.pulse.iter().enumerate() {
                    if *pulse <= 0.0 {
                        continue;
                    }
                    let src = &layout.keys[source.min(layout.len() - 1)];
                    let radius = (1.0 - *pulse) * 4.2;
                    for (i, target) in layout.keys.iter().enumerate() {
                        let dx = target.x - src.x;
                        let dy = (target.y - src.y) * 1.4;
                        let distance = (dx * dx + dy * dy).sqrt();
                        let ring = (1.0 - (distance - radius).abs() / 0.9).max(0.0);
                        let color = mix(primary, lighting.colors[7], (radius * 0.3).min(1.0));
                        let add = scale(color, ring * pulse.powf(0.6));
                        self.colors[i] = add_sat(self.colors[i], add);
                    }
                }
            }
            8 => {
                self.colors.fill(scale(primary, 0.05));
                let ignite = dt * (1.0 + speed * 1.4);
                for i in 0..layout.len() {
                    if self.sparks[i] <= 0.0 && self.rand() < ignite * 0.02 {
                        self.sparks[i] = 1.0;
                        self.spark_hue[i] = self.rand();
                    }
                }
                for i in 0..layout.len() {
                    if self.sparks[i] > 0.0 {
                        let color = hsv(self.spark_hue[i], 0.9, 1.0);
                        self.colors[i] = add_sat(self.colors[i], scale(color, self.sparks[i]));
                    }
                }
            }
            9 => {
                let band = (self.time * speed * 0.22).rem_euclid(1.35) - 0.2;
                for (i, key) in layout.keys.iter().enumerate() {
                    let phase = dir_phase(direction, center_x(key, layout), center_y(key));
                    let distance = (phase - band).abs();
                    let value = (1.0 - distance / 0.16).max(0.0).powf(1.4);
                    self.colors[i] = scale(primary, value.max(0.04));
                }
            }
            10 => {
                self.colors.fill(scale(primary, 0.05));
                let ignite = dt * (0.4 + speed * 0.5);
                for i in 0..layout.len() {
                    if self.sparks[i] <= 0.0 && self.rand() < ignite * 0.02 {
                        self.sparks[i] = 1.0;
                        self.spark_hue[i] =
                            (i as f32 / layout.len() as f32 + self.time * 0.02).rem_euclid(1.0);
                    }
                }
                for i in 0..layout.len() {
                    if self.sparks[i] > 0.0 {
                        let color = mix(primary, hsv(self.spark_hue[i], 0.8, 1.0), 0.5);
                        self.colors[i] =
                            add_sat(self.colors[i], scale(color, self.sparks[i].powf(1.3)));
                    }
                }
            }
            11 => {
                for (i, key) in layout.keys.iter().enumerate() {
                    let seed = hash01(key.col as u32 * 131 + key.row as u32 * 71);
                    let drop = (self.time * speed * 0.5 + seed * 5.0).rem_euclid(ROWS as f32 + 1.5);
                    let distance = (key.y - drop + 0.5).abs();
                    let value = if distance < 2.4 {
                        (1.0 - distance / 2.4).powf(2.2)
                    } else {
                        0.0
                    };
                    self.colors[i] = scale(primary, value.max(0.02));
                }
            }
            12 => {
                self.colors.fill([0, 0, 0]);
                self.burst_timer -= dt;
                if self.burst_timer <= 0.0 {
                    self.burst_timer = 0.55 + self.rand() * 0.9;
                    let hue = self.rand();
                    let count = 2 + (self.rand() * 3.0) as usize;
                    for _ in 0..count {
                        let x = 0.1 + self.rand() * 0.8;
                        let y = 0.1 + self.rand() * 0.8;
                        let hue = (hue + self.rand() * 0.12).rem_euclid(1.0);
                        let life = 0.9 + self.rand() * 0.7;
                        self.bursts.push(Burst {
                            x,
                            y,
                            hue,
                            age: 0.0,
                            life,
                        });
                    }
                }
                for burst in &mut self.bursts {
                    burst.age += dt;
                }
                self.bursts.retain(|b| b.age < b.life);
                for burst in &self.bursts {
                    let radius = burst.age * 1.6;
                    let fade = (1.0 - burst.age / burst.life).max(0.0);
                    for (i, key) in layout.keys.iter().enumerate() {
                        let nx = center_x(key, layout);
                        let ny = center_y(key);
                        let dx = (nx - burst.x) * 7.0;
                        let dy = (ny - burst.y) * 3.2;
                        let distance = (dx * dx + dy * dy).sqrt();
                        let ring = (1.0 - (distance - radius).abs() / 0.75).max(0.0);
                        let color = hsv(burst.hue, 0.85, 1.0);
                        self.colors[i] = add_sat(self.colors[i], scale(color, ring * fade * 0.9));
                    }
                }
            }
            _ => self.colors.fill(primary),
        }
    }

    fn custom(&mut self, lighting: &Lighting, layout: &Layout, dt: f32) {
        let anim = &lighting.animation;
        let use_animation =
            lighting.animation_playing && !anim.keyframes.is_empty() && anim.duration_ms() > 0;
        if !use_animation {
            for i in 0..layout.len() {
                self.colors[i] = *lighting.custom.keys.get(i).unwrap_or(&[0, 0, 0]);
            }
            return;
        }

        let fps = anim.fps.clamp(5, 60) as f32;
        let speed = 0.2 + anim.speed as f32 / 100.0 * 2.4;
        self.anim_time += dt * speed;
        self.anim_accum += dt;
        if self.anim_accum >= 1.0 / fps || self.anim_colors.len() != layout.len() {
            self.anim_accum = 0.0;
            self.anim_colors = self.sample_animation(anim, layout.len(), self.anim_time);
        }
        for (i, key) in layout.keys.iter().enumerate() {
            let phase = dir_phase(anim.direction, center_x(key, layout), center_y(key));
            let modulation = 0.55 + 0.45 * ((phase * 3.0 - self.anim_time * 1.4) * TAU).sin();
            self.colors[i] = scale(self.anim_colors[i], modulation.max(0.12));
        }
    }

    fn sample_animation(&self, anim: &AnimPreset, len: usize, time: f32) -> Vec<[u8; 3]> {
        let duration = anim.duration_ms() as f32;
        let mut t = (time * 1000.0).rem_euclid(duration);
        let frames = &anim.keyframes;
        let mut out = vec![[0u8, 0, 0]; len];
        if frames.is_empty() {
            return out;
        }
        let mut prev = &frames[0];
        let mut next = &frames[0];
        for (index, frame) in frames.iter().enumerate() {
            if frame.t_ms as f32 <= t {
                prev = frame;
                next = frames.get(index + 1).unwrap_or(&frames[0]);
            }
        }
        let start = prev.t_ms as f32;
        let mut end = next.t_ms as f32;
        if end <= start {
            end = start + duration;
        }
        if next.t_ms as f32 <= t {
            t += duration;
        }
        let span = (end - start).max(1.0);
        let blend = ((t - start) / span).clamp(0.0, 1.0);
        for i in 0..len {
            let a = *prev.keys.get(i).unwrap_or(&[0, 0, 0]);
            let b = *next.keys.get(i).unwrap_or(&[0, 0, 0]);
            out[i] = mix(a, b, blend);
        }
        out
    }

    fn music(&mut self, lighting: &Lighting, layout: &Layout, dt: f32, live: &LiveInputs<'_>) {
        let cfg = &lighting.music;
        let smoothing = cfg.smoothing.clamp(0.0, 0.95);
        let follow = (1.0 - smoothing) * (dt * 60.0).clamp(0.15, 2.0);
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

fn hash01(value: u32) -> f32 {
    let mut x = value.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2545_F491);
    x ^= x >> 13;
    (x & 0xFFFF) as f32 / 65535.0
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
        let mut activity = KeyActivity::new(layout.len());
        if effect >= 7 {
            activity.press(20);
        }
        let audio = AudioSnapshot::default();
        let screen = ScreenSnapshot::default();
        let live = LiveInputs {
            activity: &activity,
            audio: &audio,
            screen: &screen,
        };
        let mut last = vec![[0u8; 3]; layout.len()];
        let mut bright = false;
        for _ in 0..frames {
            last = engine.render(&lighting, layout, 0.016, &live).to_vec();
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

    #[test]
    fn music_and_screen_modes_are_safe_without_input() {
        let layout = layout();
        for mode in [
            LightingMode::Music,
            LightingMode::Screen,
            LightingMode::Custom,
        ] {
            let mut lighting = Lighting::default();
            lighting.mode = mode;
            let mut engine = RgbEngine::new();
            let activity = KeyActivity::new(layout.len());
            let audio = AudioSnapshot::default();
            let screen = ScreenSnapshot::default();
            let live = LiveInputs {
                activity: &activity,
                audio: &audio,
                screen: &screen,
            };
            let colors = engine.render(&lighting, layout, 0.016, &live);
            assert_eq!(colors.len(), 68);
        }
    }
}
