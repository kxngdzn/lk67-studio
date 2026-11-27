//! Synthetic key injection via `SendInput`.

use crate::model::action::{MOD_ALT, MOD_CTRL, MOD_SHIFT, MOD_WIN};

/// Marker written into `dwExtraInfo` of every key this app injects.
///
/// The low-level hook ignores exactly these events, so its own remaps and macro
/// playback cannot feed back into it, while keys re-injected by other keyboard
/// software are still treated as real presses.
pub const INJECT_TAG: usize = 0x4C4B_3637;

/// Extended keys need the extended flag for games and some apps to see them.
fn is_extended(vk: u16) -> bool {
    matches!(
        vk,
        0x21..=0x28 | 0x2C..=0x2E | 0x5B | 0x5C | 0x5D | 0x6F | 0x90 | 0xA3 | 0xA5 | 0xAD..=0xB3
            | 0xAC | 0x81 | 0x86
    )
}

#[cfg(windows)]
pub fn send_key(vk: u16, down: bool) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
        KEYEVENTF_KEYUP, MapVirtualKeyW, SendInput,
    };

    let mut flags: KEYBD_EVENT_FLAGS = 0;
    if !down {
        flags |= KEYEVENTF_KEYUP;
    }
    if is_extended(vk) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    let scan = unsafe { MapVirtualKeyW(vk as u32, 0) } as u16;
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: INJECT_TAG,
            },
        },
    };
    unsafe {
        SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
    }
}

#[cfg(not(windows))]
pub fn send_key(_vk: u16, _down: bool) {}

/// Press or release every modifier in a bitmask.
pub fn send_mods(bits: u8, down: bool) {
    for (bit, vk) in [
        (MOD_CTRL, 0xA2u16),
        (MOD_SHIFT, 0xA0),
        (MOD_ALT, 0xA4),
        (MOD_WIN, 0x5B),
    ] {
        if bits & bit != 0 {
            send_key(vk, down);
        }
    }
}

/// Press a modifier combo and the key, then release everything.
pub fn tap_combo(bits: u8, vk: u16) {
    send_mods(bits, true);
    send_key(vk, true);
    send_key(vk, false);
    send_mods(bits, false);
}

pub fn tap_key(vk: u16) {
    send_key(vk, true);
    send_key(vk, false);
}
