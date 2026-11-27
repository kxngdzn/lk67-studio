//! Low-level keyboard hook.
//!
//! One hook instance is installed for the lifetime of the process. Everything it needs
//! lives behind an `RwLock<HookConfig>` that the UI updates whenever the active profile
//! or settings change. Events are pushed through bounded channels with `try_send` so the
//! hook can never stall the system input queue.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, OnceLock, RwLock};

use crate::input::inject::{send_key, send_mods};
use crate::input::macro_player::{MacroPlayer, PlayerJob};
use crate::input::recorder::Recorder;
use crate::model::action::{Action, RgbCommand};
use crate::model::layout::{KEY_COUNT, layout};
use crate::model::profile::{KnobTurn, Profile, Settings};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookAction {
    PassThrough,
    Swallow,
    Key(u16),
    Combo { mods: u8, key: u16 },
    Macro(u64),
    Fn,
    Rgb(u8),
    Profile(u8),
}

impl HookAction {
    pub fn from_action(action: &Action) -> Self {
        match action {
            Action::None => HookAction::Swallow,
            Action::Key { key } => HookAction::Key(key.vk()),
            Action::Combo { mods, key } => HookAction::Combo {
                mods: mods.bits(),
                key: key.vk(),
            },
            Action::Macro { macro_id } => HookAction::Macro(*macro_id),
            Action::Media { media } => HookAction::Key(media.vk()),
            Action::Fn => HookAction::Fn,
            Action::Rgb { command } => HookAction::Rgb(rgb_command_code(*command)),
            Action::Profile { slot } => HookAction::Profile(*slot),
        }
    }
}

/// The RGB command a knob turn (volume up/down key) should run, when the knob is
/// configured for something other than plain volume.
fn knob_turn_command(turn: KnobTurn, vk: u16) -> Option<RgbCommand> {
    use RgbCommand::*;
    let up = match vk {
        0xAF => true,
        0xAE => false,
        _ => return None,
    };
    Some(match turn {
        KnobTurn::Volume => return None,
        KnobTurn::Brightness => {
            if up {
                BrightnessUp
            } else {
                BrightnessDown
            }
        }
        KnobTurn::Speed => {
            if up {
                SpeedUp
            } else {
                SpeedDown
            }
        }
        KnobTurn::Effects => {
            if up {
                NextEffect
            } else {
                PrevEffect
            }
        }
    })
}

pub fn rgb_command_code(command: crate::model::action::RgbCommand) -> u8 {
    use crate::model::action::RgbCommand::*;
    match command {
        NextEffect => 0,
        PrevEffect => 1,
        BrightnessUp => 2,
        BrightnessDown => 3,
        SpeedUp => 4,
        SpeedDown => 5,
        Toggle => 6,
        NextProfile => 7,
    }
}

#[derive(Clone)]
pub struct HookConfig {
    pub enabled: bool,
    pub remap_enabled: bool,
    pub hotkeys_enabled: bool,
    pub fn_vk: u16,
    pub knob_turn: KnobTurn,
    pub normal: [HookAction; KEY_COUNT],
    pub func: [HookAction; KEY_COUNT],
    pub index_by_vk: HashMap<u16, usize>,
}

impl Default for HookConfig {
    fn default() -> Self {
        HookConfig {
            enabled: false,
            remap_enabled: false,
            hotkeys_enabled: false,
            fn_vk: crate::model::keycode::VK_FN,
            knob_turn: KnobTurn::Volume,
            normal: [HookAction::PassThrough; KEY_COUNT],
            func: [HookAction::PassThrough; KEY_COUNT],
            index_by_vk: HashMap::new(),
        }
    }
}

impl HookConfig {
    pub fn build(settings: &Settings, profile: &Profile) -> Self {
        let layout = layout();
        let mut normal = [HookAction::PassThrough; KEY_COUNT];
        let mut func = [HookAction::PassThrough; KEY_COUNT];
        let mut index_by_vk = HashMap::with_capacity(KEY_COUNT);
        for (index, key) in layout.keys.iter().enumerate() {
            index_by_vk.insert(key.vk, index);
            normal[index] = HookAction::from_action(&profile.normal[index]);
            func[index] = HookAction::from_action(&profile.func[index]);
        }
        // The knob press arrives as mute (0xAD) on this firmware while the layout
        // models it as Play/Pause (0xB3); both must resolve to the Knob key.
        if let Some(knob) = layout.keys.iter().position(|key| key.vk == 0xB3) {
            index_by_vk.insert(0xAD, knob);
        }
        HookConfig {
            enabled: settings.hook_enabled,
            remap_enabled: settings.remap_enabled,
            hotkeys_enabled: settings.hotkeys_enabled,
            fn_vk: settings.fn_key_vk,
            knob_turn: settings.knob_turn,
            normal,
            func,
            index_by_vk,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RawKeyEvent {
    pub vk: u16,
    pub down: bool,
}

#[derive(Clone, Debug)]
pub enum HookCommand {
    SwitchProfile(u8),
    RgbCommand(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    None,
    Key(u16),
    Combo(u8, u16),
    Macro(u64),
    Fn,
    Swallow,
}

struct HookShared {
    config: Arc<RwLock<HookConfig>>,
    events: SyncSender<RawKeyEvent>,
    commands: SyncSender<HookCommand>,
    player: std::sync::mpsc::Sender<PlayerJob>,
    recorder: Arc<Recorder>,
    fn_down: AtomicBool,
    pending: std::sync::Mutex<[Pending; 256]>,
}

static SHARED: OnceLock<Arc<HookShared>> = OnceLock::new();

pub struct HookHandle {
    pub config: Arc<RwLock<HookConfig>>,
    pub events: Receiver<RawKeyEvent>,
    pub commands: Receiver<HookCommand>,
    /// Fed directly by the hook thread, so macro recording does not depend on the UI.
    pub recorder: Arc<Recorder>,
    pub error: Option<String>,
}

impl HookHandle {
    pub fn set_config(&self, config: HookConfig) {
        if let Ok(mut guard) = self.config.write() {
            *guard = config;
        }
    }
}

/// Install the hook. Returns a handle even on failure; `error` explains why.
pub fn start_hook(initial: HookConfig, player: &MacroPlayer) -> HookHandle {
    let (event_tx, event_rx) = mpsc::sync_channel(2048);
    let (cmd_tx, cmd_rx) = mpsc::sync_channel(64);
    let config = Arc::new(RwLock::new(initial));
    let recorder = Arc::new(Recorder::new());
    let shared = Arc::new(HookShared {
        config: config.clone(),
        events: event_tx,
        commands: cmd_tx,
        player: player.sender(),
        recorder: recorder.clone(),
        fn_down: AtomicBool::new(false),
        pending: std::sync::Mutex::new([Pending::None; 256]),
    });
    let error = install(&shared);
    HookHandle {
        config,
        events: event_rx,
        commands: cmd_rx,
        recorder,
        error,
    }
}

#[cfg(windows)]
fn install(shared: &Arc<HookShared>) -> Option<String> {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowsHookExW, WH_KEYBOARD_LL};

    if SHARED.set(shared.clone()).is_err() {
        debug_note("hook already installed in this process");
        return Some("keyboard hook already installed".into());
    }
    debug_note("installing hook");
    let (ready_tx, ready_rx) = mpsc::channel::<Option<String>>();
    std::thread::Builder::new()
        .name("lk67-hook".into())
        .spawn(move || {
            unsafe {
                let module = GetModuleHandleW(std::ptr::null());
                let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), module, 0);
                if hook.is_null() {
                    let code = windows_sys::Win32::Foundation::GetLastError();
                    let message = format!("SetWindowsHookExW failed (error {code})");
                    debug_note(&message);
                    let _ = ready_tx.send(Some(message));
                    return;
                }
                debug_note("hook installed");
                let _ = ready_tx.send(None);
                let mut msg: windows_sys::Win32::UI::WindowsAndMessaging::MSG = std::mem::zeroed();
                // A message loop is required for a low-level hook to stay installed.
                while windows_sys::Win32::UI::WindowsAndMessaging::GetMessageW(
                    &mut msg,
                    std::ptr::null_mut(),
                    0,
                    0,
                ) > 0
                {}
            }
        })
        .ok();
    match ready_rx.recv_timeout(std::time::Duration::from_millis(1500)) {
        Ok(error) => error,
        Err(_) => Some("keyboard hook thread did not start".into()),
    }
}

#[cfg(not(windows))]
fn install(_shared: &Arc<HookShared>) -> Option<String> {
    Some("the low-level keyboard hook is Windows-only".into())
}

/// Append one line per hook event to `%APPDATA%\LK67 Studio\hook.log` when
/// `LK67_DEBUG_KEYS` is set. Off by default so keystrokes are never logged.
#[cfg(windows)]
fn debug_log(vk: u16, down: bool, flags: u32, extra: usize) {
    use std::io::Write;
    if std::env::var_os("LK67_DEBUG_KEYS").is_none() {
        return;
    }
    let dir = crate::model::store::config_dir();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("hook.log"))
    {
        let _ = writeln!(
            file,
            "vk={vk:02X} down={} flags={flags:02X} extra={extra:016X}",
            down as u8
        );
    }
}

#[cfg(not(windows))]
fn debug_log(_vk: u16, _down: bool, _flags: u32, _extra: usize) {}

#[cfg(windows)]
fn debug_note(note: &str) {
    use std::io::Write;
    if std::env::var_os("LK67_DEBUG_KEYS").is_none() {
        return;
    }
    let dir = crate::model::store::config_dir();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("hook.log"))
    {
        let _ = writeln!(file, "-- {note}");
    }
}

#[cfg(not(windows))]
fn debug_note(_note: &str) {}

#[cfg(windows)]
unsafe extern "system" fn keyboard_proc(
    code: i32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, HC_ACTION, KBDLLHOOKSTRUCT, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN,
        WM_SYSKEYUP,
    };

    let pass = || unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };

    if code != HC_ACTION as i32 {
        return pass();
    }
    let Some(shared) = SHARED.get() else {
        return pass();
    };
    let kb = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
    let message = wparam as u32;
    let down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
    let up = message == WM_KEYUP || message == WM_SYSKEYUP;
    if !down && !up {
        return pass();
    }
    let vk = kb.vkCode as u16;

    // Only our own SendInput traffic is skipped. Keys re-injected by another
    // keyboard tool arrive flagged as injected; ignoring those is what used to
    // leave the recorder with just the Windows key.
    if kb.dwExtraInfo == crate::input::inject::INJECT_TAG {
        return pass();
    }
    debug_log(vk, down, kb.flags, kb.dwExtraInfo);
    // Record before the UI queue: a full channel or a paused window must never
    // cost a recorded keystroke.
    shared.recorder.push(vk, down);
    let _ = shared.events.try_send(RawKeyEvent { vk, down });

    let Ok(cfg) = shared.config.read() else {
        return pass();
    };
    if !cfg.enabled {
        return pass();
    }

    // Profile hotkeys: Ctrl+Alt+1..4 (checked before remapping so they always work).
    if cfg.hotkeys_enabled && down && (vk == 0x31 || vk == 0x32 || vk == 0x33 || vk == 0x34) {
        let ctrl =
            unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x11) }
                as u16
                & 0x8000
                != 0;
        let alt = unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x12) }
            as u16
            & 0x8000
            != 0;
        if ctrl && alt {
            let slot = vk - 0x31;
            let _ = shared
                .commands
                .try_send(HookCommand::SwitchProfile(slot as u8));
            return 1;
        }
    }

    // FN key: track the layer even when software remapping is off.
    if vk == cfg.fn_vk {
        shared.fn_down.store(down, Ordering::Relaxed);
        return 1;
    }

    let mut pending = match shared.pending.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let slot = &mut pending[vk as usize];

    if up {
        let previous = *slot;
        *slot = Pending::None;
        drop(pending);
        match previous {
            Pending::Key(target) => {
                send_key(target, false);
                return 1;
            }
            Pending::Combo(mods, target) => {
                send_key(target, false);
                send_mods(mods, false);
                return 1;
            }
            Pending::Macro(id) => {
                let _ = shared.player.send(PlayerJob::Release { id });
                return 1;
            }
            Pending::Fn => {
                shared.fn_down.store(false, Ordering::Relaxed);
                return 1;
            }
            Pending::Swallow => return 1,
            Pending::None => return pass(),
        }
    }

    if !cfg.remap_enabled {
        return pass();
    }

    // Knob rotation (volume up/down) can drive the LEDs instead of the volume.
    if let Some(command) = knob_turn_command(cfg.knob_turn, vk) {
        *slot = Pending::Swallow;
        drop(pending);
        let _ = shared
            .commands
            .try_send(HookCommand::RgbCommand(rgb_command_code(command)));
        return 1;
    }

    let fn_down = shared.fn_down.load(Ordering::Relaxed);
    let action = match cfg.index_by_vk.get(&vk) {
        Some(index) => {
            if fn_down {
                cfg.func[*index]
            } else {
                cfg.normal[*index]
            }
        }
        None => HookAction::PassThrough,
    };

    match action {
        HookAction::PassThrough => pass(),
        HookAction::Swallow => {
            *slot = Pending::Swallow;
            drop(pending);
            1
        }
        HookAction::Fn => {
            shared.fn_down.store(true, Ordering::Relaxed);
            *slot = Pending::Fn;
            drop(pending);
            1
        }
        HookAction::Key(target) => {
            if target == vk {
                drop(pending);
                return pass();
            }
            if *slot != Pending::None {
                // Auto-repeat while the key is held: keep swallowing, do not re-inject.
                drop(pending);
                return 1;
            }
            send_key(target, true);
            *slot = Pending::Key(target);
            drop(pending);
            1
        }
        HookAction::Combo { mods, key } => {
            if *slot != Pending::None {
                drop(pending);
                return 1;
            }
            send_mods(mods, true);
            send_key(key, true);
            *slot = Pending::Combo(mods, key);
            drop(pending);
            1
        }
        HookAction::Macro(id) => {
            let _ = shared.player.send(PlayerJob::Trigger { id });
            *slot = Pending::Macro(id);
            drop(pending);
            1
        }
        HookAction::Rgb(code) => {
            let _ = shared.commands.try_send(HookCommand::RgbCommand(code));
            *slot = Pending::Swallow;
            drop(pending);
            1
        }
        HookAction::Profile(slot_index) => {
            let _ = shared
                .commands
                .try_send(HookCommand::SwitchProfile(slot_index));
            *slot = Pending::Swallow;
            drop(pending);
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::layout::layout;

    #[test]
    fn knob_press_maps_to_the_knob_key() {
        let cfg = HookConfig::build(&Settings::default(), &Profile::new("test", 0));
        let knob = layout().index_of_vk(0xB3).expect("knob key");
        assert_eq!(cfg.index_by_vk.get(&0xAD), Some(&knob));
        assert_eq!(cfg.index_by_vk.get(&0xB3), Some(&knob));
        assert_eq!(cfg.normal[knob], HookAction::Key(0xB3));
    }

    #[test]
    fn knob_turn_commands_follow_the_setting() {
        assert_eq!(knob_turn_command(KnobTurn::Volume, 0xAF), None);
        assert_eq!(knob_turn_command(KnobTurn::Volume, 0xAE), None);
        assert_eq!(
            knob_turn_command(KnobTurn::Brightness, 0xAF),
            Some(RgbCommand::BrightnessUp)
        );
        assert_eq!(
            knob_turn_command(KnobTurn::Brightness, 0xAE),
            Some(RgbCommand::BrightnessDown)
        );
        assert_eq!(
            knob_turn_command(KnobTurn::Speed, 0xAF),
            Some(RgbCommand::SpeedUp)
        );
        assert_eq!(
            knob_turn_command(KnobTurn::Effects, 0xAE),
            Some(RgbCommand::PrevEffect)
        );
        assert_eq!(knob_turn_command(KnobTurn::Effects, 0x41), None);
    }
}
