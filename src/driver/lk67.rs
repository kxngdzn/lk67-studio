//! LK67-specific HID knowledge.
//!
//! The LK67 is a ROYUAN **yc500** board. Its control collection is the vendor
//! collection (usage page `0xFFFF`, usage `2`) and it speaks 64-byte HID
//! **feature reports with report ID 0**:
//!
//! ```text
//! report byte 0      report ID (0)
//! report bytes 1..65 data[0..64]
//! data[0]            opcode
//! data[7]            complemented sum for most commands
//! data[8]            complemented sum for LEDPARAM / SLEDPARAM
//! ```
//!
//! The checksum is `0xFF - (sum(data[0..at]) & 0xFF)`, not XOR.
//!
//! Sources: the ROYUAN protocol documentation from the sharkfin project and
//! reports from other yc500 boards with VID `0x3151`. The LK67-specific matrix
//! order is derived from the board's own keymap (see `dump_keymap`).

use serde::{Deserialize, Serialize};

use crate::model::action::Action;
pub use crate::model::layout::KEY_SLOTS;
use crate::model::layout::Layout;
use crate::model::lighting::{Direction, Lighting, LightingMode, MusicVisual};

/// GamaKay LK67 wired VID/PID.
pub const DEFAULT_VID: u16 = 0x3151;
pub const DEFAULT_PID: u16 = 0x4015;

/// Report ID of the vendor control collection. The opcode is *data* byte 0.
pub const DEFAULT_REPORT_ID: u8 = 0;
/// 64-byte data payload plus the report-ID byte.
pub const DEFAULT_REPORT_LEN: usize = 65;
/// Data bytes in one feature report.
pub const REPORT_DATA_LEN: usize = 64;
/// User-picture pages: 7 x 56 bytes = 384 = 128 RGB triples.
pub const KEY_COLOR_PAGES: usize = 7;
pub const KEY_COLOR_PAGE_LEN: usize = 56;
/// Largest key count the firmware's user-picture buffer holds.
pub const USERPIC_KEYS: usize = 128;

/// How vendor packets leave the host.
///
/// The LK67 vendor interface declares a 65-byte feature report and no output
/// report, so `WriteFile` fails with ERROR_ACCESS_DENIED. Use feature reports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportTransport {
    #[default]
    FeatureReport,
    OutputReport,
}

impl ReportTransport {
    pub const ALL: [ReportTransport; 2] = [
        ReportTransport::FeatureReport,
        ReportTransport::OutputReport,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ReportTransport::FeatureReport => "Feature report (HID SetFeature)",
            ReportTransport::OutputReport => "Output report (WriteFile)",
        }
    }
}

/// Vendor command opcodes for the yc500 family.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Opcodes {
    /// Write one base-layer slot (`0x13`).
    #[serde(default = "op_keymap")]
    pub keymap: u8,
    /// Write one Fn-layer slot (`0x15`).
    #[serde(default = "op_fn_keymap")]
    pub fn_keymap: u8,
    /// Upload all key colours, 7 pages (`0x0C`).
    #[serde(default = "op_key_colors")]
    pub key_colors: u8,
    /// Set hardware lighting mode (`0x07`).
    #[serde(default = "op_effect")]
    pub effect: u8,
    /// Select profile slot (`0x05`).
    #[serde(default = "op_profile")]
    pub profile: u8,
    /// Live music band stream (`0x0D`): 42 heights, 0..=6, every ~20 ms.
    #[serde(default = "op_music_stream")]
    pub music_stream: u8,
    /// Live screen colour stream (`0x0E`): one RGB colour for the whole board.
    #[serde(default = "op_screen_stream")]
    pub screen_stream: u8,
}

fn op_keymap() -> u8 {
    0x13
}
fn op_fn_keymap() -> u8 {
    0x15
}
fn op_key_colors() -> u8 {
    0x0C
}
fn op_effect() -> u8 {
    0x07
}
fn op_profile() -> u8 {
    0x05
}
fn op_music_stream() -> u8 {
    0x0D
}
fn op_screen_stream() -> u8 {
    0x0E
}

impl Default for Opcodes {
    fn default() -> Self {
        Opcodes {
            keymap: op_keymap(),
            fn_keymap: op_fn_keymap(),
            key_colors: op_key_colors(),
            effect: op_effect(),
            profile: op_profile(),
            music_stream: op_music_stream(),
            screen_stream: op_screen_stream(),
        }
    }
}

/// Read-only opcodes used by the probe.
pub const GET_LEDPARAM: u8 = 0x87;
pub const GET_KEYMAP: u8 = 0x89;
pub const GET_FN: u8 = 0x90;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DriverConfig {
    #[serde(default = "default_vid")]
    pub vid: u16,
    #[serde(default = "default_pid")]
    pub pid: u16,
    /// HID interface number to open, or -1 for the first matching interface.
    #[serde(default = "default_interface")]
    pub interface: i32,
    /// Exact HID collection to open (usage page + usage). 0 = auto-pick.
    #[serde(default)]
    pub usage_page: u16,
    #[serde(default)]
    pub usage: u16,
    #[serde(default)]
    pub transport: ReportTransport,
    /// HID report ID. The LK67 uses 0: the command opcode is data byte 0.
    #[serde(default = "default_report_id")]
    pub report_id: u8,
    #[serde(default = "default_report_len")]
    pub report_len: usize,
    #[serde(default)]
    pub opcodes: Opcodes,
    /// Number of profile slots the firmware understands (LK67: 4).
    #[serde(default = "default_slots")]
    pub profile_slots: u8,
}

fn default_vid() -> u16 {
    DEFAULT_VID
}
fn default_pid() -> u16 {
    DEFAULT_PID
}
fn default_interface() -> i32 {
    -1
}
fn default_report_id() -> u8 {
    DEFAULT_REPORT_ID
}
fn default_report_len() -> usize {
    DEFAULT_REPORT_LEN
}
fn default_slots() -> u8 {
    4
}

impl Default for DriverConfig {
    fn default() -> Self {
        DriverConfig {
            vid: default_vid(),
            pid: default_pid(),
            interface: -1,
            usage_page: 0,
            usage: 0,
            transport: ReportTransport::default(),
            report_id: default_report_id(),
            report_len: default_report_len(),
            opcodes: Opcodes::default(),
            profile_slots: default_slots(),
        }
    }
}

impl DriverConfig {
    pub fn normalize(&mut self) {
        // The vendor collection always exposes 64 data bytes; shorter reports
        // are rejected by HidD_SetFeature.
        self.report_len = self.report_len.clamp(DEFAULT_REPORT_LEN, 1024);
        self.profile_slots = self.profile_slots.clamp(1, 4);
        if self.vid == 0 && self.pid == 0 {
            self.vid = DEFAULT_VID;
            self.pid = DEFAULT_PID;
        }
    }
}

/// `0xFF - sum(bytes)` as used by every yc500 command.
fn complement_sum(bytes: &[u8]) -> u8 {
    0xFFu8.wrapping_sub(bytes.iter().fold(0u8, |acc, b| acc.wrapping_add(*b)))
}

/// Build the 65-byte HID report from 64 data bytes.
///
/// The checksum is written at `checksum_at` (data index 7 for most commands,
/// 8 for LEDPARAM/SLEDPARAM) and covers every data byte before it.
pub fn packet(cfg: &DriverConfig, mut data: [u8; REPORT_DATA_LEN], checksum_at: usize) -> Vec<u8> {
    if checksum_at > 0 && checksum_at < data.len() {
        data[checksum_at] = complement_sum(&data[..checksum_at]);
    }
    let mut report = vec![0u8; cfg.report_len.max(DEFAULT_REPORT_LEN)];
    report[0] = cfg.report_id;
    report[1..1 + REPORT_DATA_LEN].copy_from_slice(&data);
    report
}

fn empty_data() -> [u8; REPORT_DATA_LEN] {
    [0u8; REPORT_DATA_LEN]
}

pub(crate) fn direction_option(direction: Direction) -> u8 {
    match direction {
        Direction::Right | Direction::Clockwise => 0,
        Direction::Left | Direction::CounterClockwise => 1,
        Direction::Up => 2,
        Direction::Down => 3,
    }
}

/// Map the app's effect list to yc500 wire modes.
///
/// The LK67 firmware exposes the 1..19 family. Effects that have no direct
/// equivalent reuse the closest firmware animation. Music and screen modes
/// are rendered by the firmware from a host stream, so they pick modes 20/22
/// and 21 instead of the flash-backed user-picture mode 13.
pub(crate) fn wire_mode(lighting: &Lighting) -> u8 {
    match lighting.mode {
        LightingMode::Custom => return 13, // USERPIC per-key picture
        LightingMode::Music => {
            // 20 "Classic", 22 "EDM"; both draw host-fed bars.
            return match lighting.music.visual {
                MusicVisual::Pulse | MusicVisual::Twinkle => 22,
                _ => 20,
            };
        }
        LightingMode::Screen => return 21, // screen colour follow
        LightingMode::Hardware => {}
    }
    let effect = lighting.effect;
    match effect {
        0 => 1,   // "Off": static black below
        1 => 1,   // static
        2 => 2,   // breathing
        3 => 4,   // wave, rainbow
        4 => 3,   // spectrum
        5 => 4,   // wave, fixed colour
        6 => 7,   // flow
        7 => 5,   // ripple
        8 => 6,   // star dots
        9 => 9,   // layers
        10 => 17, // snowfall
        11 => 18, // meteor
        12 => 14, // radiant
        _ => 1,
    }
}

pub(crate) fn uses_direction(effect: usize) -> bool {
    matches!(effect, 3 | 5 | 6 | 7 | 9 | 10 | 11)
}

pub(crate) fn hardware_rgb(lighting: &Lighting) -> [u8; 3] {
    if lighting.effect == 0 {
        return [0, 0, 0];
    }
    let mut rgb = lighting.colors[0];
    // The firmware treats values below 0x08 per channel as off.
    for channel in &mut rgb {
        if *channel < 0x08 {
            *channel = 0;
        }
    }
    // White is sent as 0xFAFAFA by the vendor software.
    if rgb == [255, 255, 255] {
        rgb = [0xFA, 0xFA, 0xFA];
    }
    rgb
}

fn brightness_byte(lighting: &Lighting) -> u8 {
    ((lighting.brightness.min(100) as u16 * 4) / 100) as u8
}

fn speed_byte(lighting: &Lighting) -> u8 {
    // Host 0..100 -> wire 5..1 (inverted).
    5u8.saturating_sub(((lighting.speed.min(100) as u16 * 4) / 100) as u8)
        .max(1)
}

/// SET_LEDPARAM (`0x07`), Bit8 checksum.
///
/// `[op, mode, speed, brightness, (option<<4)|flags, R, G, B, ck8]`
pub fn build_effect_report(cfg: &DriverConfig, lighting: &Lighting) -> Vec<u8> {
    let mode = wire_mode(lighting);
    let mut data = empty_data();
    data[0] = cfg.opcodes.effect;
    data[1] = mode;
    data[2] = speed_byte(lighting);
    data[3] = brightness_byte(lighting);
    // Mode 20/22 (music) and 21 (screen) walk their own palette: 0 = rainbow.
    // Only "Rainbow wave" and "Rainbow cycle" ask the firmware for a rainbow;
    // every other effect has to carry the palette colour, or the board falls
    // back to its stock preset (red/pink) and ignores the chosen colour.
    let flags = match lighting.mode {
        LightingMode::Music | LightingMode::Screen => 0,
        _ if matches!(lighting.effect, 3 | 4) => 8,
        _ => 7,
    };
    let option = if uses_direction(lighting.effect) {
        direction_option(lighting.direction)
    } else {
        0
    };
    data[4] = (option << 4) | flags;
    let rgb = if matches!(lighting.mode, LightingMode::Hardware) {
        hardware_rgb(lighting)
    } else {
        [0, 0, 0]
    };
    data[5..8].copy_from_slice(&rgb);
    packet(cfg, data, 8)
}

/// GET_LEDPARAM (`0x87`). The reply echoes the opcode in data byte 0.
pub fn build_get_led_report(cfg: &DriverConfig) -> Vec<u8> {
    let mut data = empty_data();
    data[0] = GET_LEDPARAM;
    packet(cfg, data, 7)
}

/// One GET page of the active profile's base (`0x89`) or Fn (`0x90`) keymap.
pub fn build_get_keymap_page(cfg: &DriverConfig, profile: u8, page: u8, func: bool) -> Vec<u8> {
    let mut data = empty_data();
    data[0] = if func { GET_FN } else { GET_KEYMAP };
    data[1] = profile;
    data[2] = page.min(7);
    packet(cfg, data, 7)
}

/// Chunk per-key colours into seven USERPIC pages of 56 bytes.
///
/// yc500 header: `[0x0C, 0, len_lo, len_hi, page, 0]`, data at byte 8.
pub fn build_key_color_reports(cfg: &DriverConfig, colors: &[[u8; 3]]) -> Vec<Vec<u8>> {
    let rgb = matrix_colors(colors);
    let bytes: Vec<u8> = rgb.concat();
    let total = bytes.len() as u16;
    (0..KEY_COLOR_PAGES)
        .map(|page| {
            let mut data = empty_data();
            data[0] = cfg.opcodes.key_colors;
            data[1] = 0;
            data[2] = (total & 0xFF) as u8;
            data[3] = (total >> 8) as u8;
            data[4] = page as u8;
            data[5] = 0;
            let start = page * KEY_COLOR_PAGE_LEN;
            let end = (start + KEY_COLOR_PAGE_LEN).min(bytes.len());
            data[8..8 + (end - start)].copy_from_slice(&bytes[start..end]);
            packet(cfg, data, 7)
        })
        .collect()
}

/// Live music bands (`0x0D`): 42 heights, each 0..=6, at about 50 Hz.
///
/// The keyboard draws them in modes 20 (Classic) and 22 (EDM). Nothing here
/// touches flash, so this is the only live per-key path the board really has.
pub const MUSIC_HEIGHTS: usize = 42;

pub fn build_music_report(cfg: &DriverConfig, heights: &[u8; MUSIC_HEIGHTS]) -> Vec<u8> {
    let mut data = empty_data();
    data[0] = cfg.opcodes.music_stream;
    for (index, height) in heights.iter().enumerate() {
        data[8 + index] = (*height).min(6);
    }
    packet(cfg, data, 7)
}

/// Live screen colour (`0x0E`): the firmware paints the whole board with it.
pub fn build_screen_color_report(cfg: &DriverConfig, rgb: [u8; 3]) -> Vec<u8> {
    let mut data = empty_data();
    data[0] = cfg.opcodes.screen_stream;
    data[1..4].copy_from_slice(&rgb);
    packet(cfg, data, 7)
}

pub fn build_profile_report(cfg: &DriverConfig, slot: u8) -> Vec<u8> {
    let mut data = empty_data();
    data[0] = cfg.opcodes.profile;
    data[1] = slot.min(cfg.profile_slots.saturating_sub(1));
    packet(cfg, data, 7)
}

/// Convert a Windows VK code to a HID keyboard/consumer usage.
pub fn vk_to_hid(vk: u16) -> Option<u8> {
    let usage = match vk {
        0x1B => 0x29,                            // Esc
        0x08 => 0x2A,                            // Backspace
        0x09 => 0x2B,                            // Tab
        0x20 => 0x2C,                            // Space
        0xBD => 0x2D,                            // -
        0xBB => 0x2E,                            // =
        0xDB => 0x2F,                            // [
        0xDD => 0x30,                            // ]
        0xDC => 0x31,                            // backslash
        0xBA => 0x33,                            // ;
        0xDE => 0x34,                            // '
        0xC0 => 0x35,                            // `
        0xBC => 0x36,                            // ,
        0xBE => 0x37,                            // .
        0xBF => 0x38,                            // /
        0x14 => 0x39,                            // CapsLock
        0x70..=0x7B => 0x3A + (vk - 0x70) as u8, // F1..F12
        0x7C..=0x87 => 0x68 + (vk - 0x7C) as u8, // F13..F24
        0x2C => 0x46,                            // PrintScreen
        0x91 => 0x47,                            // ScrollLock
        0x13 => 0x48,                            // Pause
        0x2D => 0x49,                            // Insert
        0x24 => 0x4A,                            // Home
        0x21 => 0x4B,                            // PageUp
        0x2E => 0x4C,                            // Delete
        0x23 => 0x4D,                            // End
        0x22 => 0x4E,                            // PageDown
        0x27 => 0x4F,                            // Right
        0x25 => 0x50,                            // Left
        0x28 => 0x51,                            // Down
        0x26 => 0x52,                            // Up
        0x90 => 0x53,                            // NumLock
        0x6F => 0x54,                            // Num /
        0x6A => 0x55,                            // Num *
        0x6D => 0x56,                            // Num -
        0x6B => 0x57,                            // Num +
        0x0D => 0x28,
        0x60 => 0x62, // Num 0
        0x61..=0x69 => 0x59 + (vk - 0x61) as u8,
        0x6E => 0x63, // Num .
        0x5D => 0x65, // Menu
        0xA0 => 0xE1, // LShift
        0xA1 => 0xE5, // RShift
        0xA2 => 0xE0, // LCtrl
        0xA3 => 0xE4, // RCtrl
        0xA4 => 0xE2, // LAlt
        0xA5 => 0xE6, // RAlt
        0x5B => 0xE3, // LWin
        0x5C => 0xE7, // RWin
        0x31..=0x39 => 0x1E + (vk - 0x31) as u8,
        0x30 => 0x27, // 0
        0x41..=0x5A => 0x04 + (vk - 0x41) as u8,
        _ => return None,
    };
    Some(usage)
}

/// Map a Windows VK code to a HID consumer-page usage (16-bit).
pub fn vk_to_consumer(vk: u16) -> Option<u16> {
    Some(match vk {
        0xAD => 0x00E2, // mute
        0xAE => 0x00EA, // volume down
        0xAF => 0x00E9, // volume up
        0xB0 => 0x00B5, // next track
        0xB1 => 0x00B6, // previous track
        0xB2 => 0x00B7, // stop
        0xB3 => 0x00CD, // play/pause
        _ => return None,
    })
}

/// Four-byte keymap slot value.
pub fn action_entry(action: &Action) -> [u8; 4] {
    match action {
        Action::None => [0; 4],
        Action::Key { key } => match vk_to_hid(key.vk()) {
            Some(usage) => [0x00, 0x00, usage, 0x00],
            None => [0; 4],
        },
        Action::Combo { mods, key } => {
            let usage = vk_to_hid(key.vk()).unwrap_or(0);
            [0x00, mods.bits(), 0, usage]
        }
        Action::Macro { macro_id } => [0x09, 0, (*macro_id & 0xFF) as u8, 0],
        Action::Media { media } => match vk_to_consumer(media.vk()) {
            Some(usage) => [0x03, 0x00, (usage & 0xFF) as u8, (usage >> 8) as u8],
            None => [0; 4],
        },
        Action::Fn => [0x0A, 0x01, 0, 0],
        Action::Rgb { command } => [0x0A, 0x00, rgb_command_code(*command), 0],
        Action::Profile { slot } => [0x0A, 0x00, 0x10 + (*slot).min(3), 0],
    }
}

fn rgb_command_code(command: crate::model::action::RgbCommand) -> u8 {
    use crate::model::action::RgbCommand::*;
    match command {
        NextEffect => 1,
        PrevEffect => 2,
        BrightnessUp => 3,
        BrightnessDown => 4,
        SpeedUp => 5,
        SpeedDown => 6,
        Toggle => 7,
        NextProfile => 8,
    }
}

/// One keymap slot write (base `0x13` / Fn `0x15`), Bit7 checksum.
///
/// `[op, profile, slot, 0, 0, 0, 0, ck7, type, b1, b2, b3, ...]`
pub fn build_keymap_report(
    cfg: &DriverConfig,
    profile: u8,
    layer_func: bool,
    slot: u16,
    action: &Action,
) -> Vec<u8> {
    let mut data = empty_data();
    data[0] = if layer_func {
        cfg.opcodes.fn_keymap
    } else {
        cfg.opcodes.keymap
    };
    data[1] = profile;
    data[2] = slot.min(127) as u8;
    data[8..12].copy_from_slice(&action_entry(action));
    packet(cfg, data, 7)
}

/// Number of keymap writes needed for a full profile (changed keys only).
pub fn changed_keys(
    layout: &Layout,
    normal: &[Action],
    func: &[Action],
) -> Vec<(bool, usize, Action)> {
    let default_normal = layout.default_normal();
    let default_func = layout.default_func();
    let mut out = Vec::new();
    for (index, (current, default)) in normal.iter().zip(default_normal.iter()).enumerate() {
        if current != default {
            out.push((false, index, current.clone()));
        }
    }
    for (index, (current, default)) in func.iter().zip(default_func.iter()).enumerate() {
        if current != default {
            out.push((true, index, current.clone()));
        }
    }
    out
}

/// Number of LEDs the firmware knows.
pub const LED_SLOTS: usize = USERPIC_KEYS;

/// Reorder app key colours into firmware matrix order (128 slots).
pub fn matrix_colors(colors: &[[u8; 3]]) -> Vec<[u8; 3]> {
    let mut out = vec![[0u8; 3]; LED_SLOTS];
    for (index, color) in colors.iter().take(KEY_SLOTS.len()).enumerate() {
        let slot = KEY_SLOTS[index] as usize;
        if slot < out.len() {
            out[slot] = *color;
        }
    }
    out
}

/// Compact hex dump for the packet inspector.
pub fn hex_dump(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for (i, byte) in bytes.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&format!("{byte:02X}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::keycode::KeyCode;

    #[test]
    fn reports_use_report_id_zero_and_opcode_in_data() {
        let cfg = DriverConfig::default();
        let report = build_effect_report(&cfg, &Lighting::default());
        assert_eq!(report.len(), 65);
        assert_eq!(report[0], 0);
        assert_eq!(report[1], 0x07);
        // Bit8 checksum covers data bytes 0..8.
        let sum = report[1..9].iter().fold(0u8, |acc, b| acc.wrapping_add(*b));
        assert_eq!(report[9], 0xFFu8.wrapping_sub(sum));
    }

    #[test]
    fn key_colors_use_seven_pages_and_384_bytes() {
        let cfg = DriverConfig::default();
        let reports = build_key_color_reports(&cfg, &vec![[1u8, 2, 3]; 68]);
        assert_eq!(reports.len(), 7);
        assert_eq!(reports[0][1], 0x0C);
        assert_eq!(reports[0][3], 0x80); // 384 low byte
        assert_eq!(reports[0][4], 0x01); // 384 high byte
        assert_eq!(reports[6][5], 6); // page index
        for report in &reports {
            assert_eq!(report.len(), 65);
            assert_eq!(report[0], 0);
        }
    }

    #[test]
    fn live_modes_use_host_fed_led_modes() {
        let cfg = DriverConfig::default();
        let mut lighting = Lighting::default();

        lighting.mode = LightingMode::Custom;
        assert_eq!(build_effect_report(&cfg, &lighting)[2], 13);

        lighting.mode = LightingMode::Screen;
        assert_eq!(build_effect_report(&cfg, &lighting)[2], 21);

        lighting.mode = LightingMode::Music;
        lighting.music.visual = MusicVisual::Equalizer;
        assert_eq!(build_effect_report(&cfg, &lighting)[2], 20);
        lighting.music.visual = MusicVisual::Pulse;
        assert_eq!(build_effect_report(&cfg, &lighting)[2], 22);
    }

    #[test]
    fn reactive_effects_carry_the_palette_colour() {
        let cfg = DriverConfig::default();
        let mut lighting = Lighting::default();
        lighting.mode = LightingMode::Hardware;
        lighting.effect = 8; // Reactive sparkle
        lighting.colors[0] = [10, 200, 30];
        let report = build_effect_report(&cfg, &lighting);
        assert_eq!(report[5] & 0x0F, 7, "fixed-colour flag");
        assert_eq!(&report[6..9], &[10, 200, 30]);

        // Rainbow effects keep asking the firmware for its own colours.
        lighting.effect = 3;
        assert_eq!(build_effect_report(&cfg, &lighting)[5] & 0x0F, 8);
    }

    #[test]
    fn music_and_screen_stream_packets_are_shaped() {
        let cfg = DriverConfig::default();
        let mut heights = [0u8; MUSIC_HEIGHTS];
        heights[0] = 6;
        heights[41] = 3;
        let music = build_music_report(&cfg, &heights);
        assert_eq!(music[0], 0);
        assert_eq!(music[1], 0x0D);
        assert_eq!(music[9], 6);
        assert_eq!(music[50], 3);
        let sum = music[1..8].iter().fold(0u8, |acc, b| acc.wrapping_add(*b));
        assert_eq!(music[8], 0xFFu8.wrapping_sub(sum));

        let screen = build_screen_color_report(&cfg, [12, 34, 56]);
        assert_eq!(screen[1], 0x0E);
        assert_eq!(&screen[2..5], &[12, 34, 56]);
    }

    #[test]
    fn keymap_has_profile_slot_and_entry_at_byte_8() {
        let cfg = DriverConfig::default();
        let report = build_keymap_report(&cfg, 2, false, 41, &Action::Key { key: KeyCode(0x41) });
        assert_eq!(report[1], 0x13);
        assert_eq!(report[2], 2);
        assert_eq!(report[3], 41);
        assert_eq!(report[9], 0x00);
        assert_eq!(report[10], 0x00);
        assert_eq!(report[11], 0x04); // HID usage for A
    }
}
