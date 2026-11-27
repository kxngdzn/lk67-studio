//! Application shell: owns all live state and drives the UI, driver and RGB engine.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use eframe::egui;

use crate::driver::{DeviceStatus, HidRecord, KeyboardDriver, Lk67Driver};
use crate::input::{
    HookCommand, HookConfig, HookHandle, KnobEvent, KnobWatcher, MacroPlayer, PlayerStatus,
    RawKeyEvent, start_hook,
};
use crate::model::action::RgbCommand;
use crate::model::layout::{KEY_COUNT, layout};
use crate::model::lighting::{AudioSource, LightingMode, ScreenConfig};
use crate::model::profile::KnobTurn;
use crate::model::{Action, Config, Layer};
use crate::rgb::{
    AudioEngine, AudioSnapshot, KeyActivity, LiveInputs, RgbEngine, ScreenEngine, ScreenSnapshot,
};
use crate::ui::keymap::{KeymapView, show as keymap_show};
use crate::ui::{Page, Tool, UiState, theme, widgets};

pub struct App {
    pub cfg: Config,
    pub driver: Lk67Driver,
    pub engine: RgbEngine,
    pub activity: KeyActivity,
    pub audio: AudioEngine,
    pub screen: ScreenEngine,
    pub hook: HookHandle,
    pub knob: KnobWatcher,
    pub player: MacroPlayer,
    pub colors: Vec<[u8; 3]>,
    pub ui: UiState,
    pub audio_snap: AudioSnapshot,
    pub screen_snap: ScreenSnapshot,
    pub devices: Vec<HidRecord>,
    pub last_frame: Instant,
    pub save_due: Option<Instant>,
    pub saved_at: Option<Instant>,
    pub last_stream_at: Instant,
    pub last_screen_cfg: Option<ScreenConfig>,
    pub last_audio_source: Option<AudioSource>,
    /// Result of the background "clear onboard remaps" job.
    pub restore_rx: Option<std::sync::mpsc::Receiver<Result<usize, String>>>,
    /// Result of the background per-key picture upload.
    pub upload_rx: Option<std::sync::mpsc::Receiver<Result<usize, String>>>,
    /// Last streaming error, so a stalled write is reported once, not per frame.
    pub last_stream_error: Option<String>,
    /// Previously seen output mode; entering music/screen starts its capture.
    pub last_lighting_mode: Option<LightingMode>,
    /// Tray icon, when the platform accepted one.
    pub tray: Option<crate::tray::Tray>,
    /// Set by the tray's Quit item so the close request is not turned into a hide.
    pub quitting: Arc<AtomicBool>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let (mut cfg, notes) = Config::load();
        cfg.normalize();
        // Earlier builds exposed the LK67's knob press as "End". Replace that
        // saved placeholder so applying a keymap cannot overwrite slot 75.
        if let Some(knob) = layout().index_of_vk(0xB3) {
            let default = layout().default_normal()[knob].clone();
            for profile in &mut cfg.profiles {
                for layer in [&mut profile.normal, &mut profile.func] {
                    if matches!(
                        layer.get(knob),
                        Some(Action::Key { key }) if key.vk() == 0x23
                    ) {
                        layer[knob] = default.clone();
                    }
                }
            }
        }

        let player = MacroPlayer::new();
        let hook = start_hook(HookConfig::build(&cfg.settings, cfg.profile()), &player);
        let knob = crate::input::knob::start(cfg.driver.vid, cfg.driver.pid);
        hook.recorder.set_repaint_context(cc.egui_ctx.clone());
        let mut driver = Lk67Driver::new(cfg.driver.clone(), cfg.settings.dry_run);
        if !cfg.settings.dry_run {
            driver.rescan();
        }
        let devices = driver.discover();

        let mut ui = UiState::default();
        ui.paint_color = cfg.profile().lighting.colors[0];
        ui.selected_macro = cfg.profile().macros.first().map(|m| m.id);
        match &hook.error {
            Some(err) => ui.notify(format!("Keyboard hook unavailable: {err}")),
            None => {
                if let Some(note) = notes.last() {
                    ui.notify(note.clone());
                }
            }
        }

        let app = App {
            cfg,
            driver,
            engine: RgbEngine::new(),
            activity: KeyActivity::new(KEY_COUNT),
            audio: AudioEngine::new(),
            screen: ScreenEngine::new(),
            hook,
            knob,
            player,
            colors: vec![[0, 0, 0]; KEY_COUNT],
            ui,
            audio_snap: AudioSnapshot::default(),
            screen_snap: ScreenSnapshot::default(),
            devices,
            last_frame: Instant::now(),
            save_due: None,
            saved_at: None,
            last_stream_at: Instant::now() - Duration::from_secs(1),
            last_screen_cfg: None,
            last_audio_source: None,
            restore_rx: None,
            upload_rx: None,
            last_stream_error: None,
            last_lighting_mode: None,
            tray: None,
            quitting: Arc::new(AtomicBool::new(false)),
        };
        app.player.sync_macros(&app.cfg.profile().macros);
        // Push the active profile's lighting on launch: the board keeps whichever
        // mode it was last left in, including host-fed music/screen modes.
        let mut app = app;
        app.ui.apply_lighting = true;
        app.tray = crate::tray::build(&cc.egui_ctx, app.quitting.clone());
        if app.tray.is_none() {
            app.ui
                .notify("Tray icon unavailable; the window closes normally");
        }
        app
    }

    pub fn profile_mut(&mut self) -> &mut crate::model::Profile {
        self.cfg.profile_mut()
    }

    pub fn notify(&mut self, message: impl Into<String>) {
        self.ui.notify(message);
    }

    pub fn save_now(&mut self) {
        match self.cfg.save() {
            Ok(path) => {
                self.saved_at = Some(Instant::now());
                self.ui.notify(format!("Saved {}", path.display()));
            }
            Err(err) => self.ui.notify(format!("Save failed: {err}")),
        }
    }

    pub fn switch_profile(&mut self, slot: usize) {
        if slot >= self.cfg.profiles.len() || slot == self.cfg.active_profile {
            return;
        }
        self.cfg.active_profile = slot;
        let profile = self.cfg.profile();
        self.player.sync_macros(&profile.macros);
        self.hook
            .set_config(HookConfig::build(&self.cfg.settings, profile));
        self.ui.selected = None;
        self.ui.selected_macro = self.cfg.profile().macros.first().map(|m| m.id);
        self.ui.paint_color = self.cfg.profile().lighting.colors[0];
        self.ui.fn_held = false;
        self.last_screen_cfg = None;
        self.last_audio_source = None;
        // Firmware profile slots belong to the onboard keymap. Selecting one
        // while software remap is active would surface old onboard remaps and
        // apply them on top of the hook, so that path stays untouched.
        if self.cfg.settings.sync_profile_to_hardware && !self.onboard_keymap_active() {
            let _ = self.driver.select_profile(slot as u8);
        }
        // Lighting is independent of remapping: every profile switch updates the
        // keyboard immediately.
        self.ui.apply_lighting = true;
        self.ui.dirty = true;
        self.ui
            .notify(format!("Profile {}: {}", slot + 1, self.cfg.profile().name));
    }

    /// The onboard keymap is the remap path only while software remapping is off.
    pub fn onboard_keymap_active(&self) -> bool {
        !self.cfg.settings.remap_enabled
    }

    /// Queue an onboard keymap write, refusing while software remap is active so
    /// a key is never remapped by both the firmware and the low-level hook.
    pub fn request_keymap_write(&mut self) -> bool {
        if !self.onboard_keymap_active() {
            self.ui.notify(
                "Software remap is active: onboard keymap not written (one remap path at a time). Turn it off in Settings to persist keys on the keyboard.",
            );
            return false;
        }
        self.ui.apply_keymap = true;
        true
    }

    /// Restore the board's stock keymap in every firmware profile slot. Old
    /// onboard swaps would otherwise keep shadowing the software map.
    ///
    /// Runs in the background: reading four slots and rewriting mismatches takes
    /// seconds and must not block the UI thread.
    pub fn clear_onboard_keymap(&mut self) {
        if self.ui.restoring {
            self.ui.notify("Onboard keymap restore is already running");
            return;
        }
        if self.cfg.settings.dry_run {
            self.ui
                .notify("Dry-run is enabled: the onboard keymap is not read or written");
            return;
        }
        let cfg = self.cfg.driver.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.restore_rx = Some(rx);
        self.ui.restoring = true;
        let spawned = std::thread::Builder::new()
            .name("lk67-restore-keymap".into())
            .spawn(move || {
                let mut driver = Lk67Driver::new(cfg, false);
                let result = driver
                    .restore_stock_keymap(crate::model::layout::layout())
                    .map_err(|err| err.to_string());
                let _ = tx.send(result);
            });
        if let Err(err) = spawned {
            self.restore_rx = None;
            self.ui.restoring = false;
            self.ui
                .notify(format!("Could not start keymap restore: {err}"));
        } else {
            self.ui
                .notify("Clearing onboard remaps in the background...");
        }
    }

    /// Upload the painted per-key picture and switch the board to it.
    ///
    /// The upload is seven flash pages with settle delays (~3 s), so it runs on
    /// a worker thread and the UI keeps repainting. Its own HID handle is used;
    /// the main driver pauses lighting writes until the job reports back.
    pub fn upload_custom_layout(&mut self) {
        if self.ui.lighting_uploading {
            self.ui
                .notify("A per-key picture upload is already running");
            return;
        }
        // Sending the picture is also the request to show it, so the profile and
        // the board agree on the output mode afterwards.
        let profile = self.cfg.profile_mut();
        profile.lighting.mode = LightingMode::Custom;
        profile.lighting.animation_playing = false;
        let lighting = profile.lighting.clone();
        self.ui.dirty = true;
        let pages =
            crate::driver::lk67::build_key_color_reports(&self.cfg.driver, &lighting.custom.keys)
                .len();
        if self.cfg.settings.dry_run {
            match self.driver.upload_custom_layout(&lighting) {
                Ok(count) => self.ui.notify(format!(
                    "Dry-run: {count} packet(s) logged, nothing written"
                )),
                Err(err) => self.ui.notify(format!("Per-key upload failed: {err}")),
            }
            return;
        }
        let cfg = self.cfg.driver.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.upload_rx = Some(rx);
        self.ui.lighting_uploading = true;
        let spawned = std::thread::Builder::new()
            .name("lk67-picture-upload".into())
            .spawn(move || {
                let mut driver = Lk67Driver::new(cfg, false);
                let result = driver
                    .upload_custom_layout(&lighting)
                    .map_err(|err| err.to_string());
                let _ = tx.send(result);
            });
        if let Err(err) = spawned {
            self.upload_rx = None;
            self.ui.lighting_uploading = false;
            self.ui
                .notify(format!("Could not start the picture upload: {err}"));
        } else {
            self.ui
                .notify(format!("Sending per-key picture ({pages} pages, ~3 s)..."));
        }
    }

    /// 42 firmware music bars from the 16 FFT bands, each 0..=6.
    fn music_heights(&self) -> [u8; crate::driver::lk67::MUSIC_HEIGHTS] {
        let bands = &self.audio_snap.bands;
        let level = self.audio_snap.level;
        let mut heights = [0u8; crate::driver::lk67::MUSIC_HEIGHTS];
        let last = bands.len() - 1;
        for (index, height) in heights.iter_mut().enumerate() {
            let pos = index as f32 * last as f32 / (crate::driver::lk67::MUSIC_HEIGHTS - 1) as f32;
            let low = pos.floor() as usize;
            let high = (low + 1).min(last);
            let t = pos - low as f32;
            let band = bands[low] * (1.0 - t) + bands[high] * t;
            let value = (band * 1.15 + level * 0.35).clamp(0.0, 1.0);
            *height = (value * 6.0).round() as u8;
        }
        heights
    }

    /// One colour for the whole board from the screen-follow grid.
    fn screen_color(&self) -> [u8; 3] {
        let grid = &self.screen_snap.grid;
        if grid.is_empty() || !self.screen_snap.active {
            return [20, 30, 45];
        }
        let mut sum = [0u32; 3];
        for cell in grid {
            for channel in 0..3 {
                sum[channel] += cell[channel] as u32;
            }
        }
        let count = grid.len() as u32;
        let mut rgb = [
            (sum[0] / count) as f32,
            (sum[1] / count) as f32,
            (sum[2] / count) as f32,
        ];
        // Dark scenes must still be visible on the LEDs.
        let peak = rgb.iter().fold(0.0f32, |acc, value| acc.max(*value));
        let gain = if peak > 1.0 {
            (200.0 / peak).clamp(1.0, 3.2)
        } else {
            1.0
        };
        for channel in &mut rgb {
            *channel = (*channel * gain).min(255.0);
        }
        [rgb[0] as u8, rgb[1] as u8, rgb[2] as u8]
    }

    /// Apply an RGB command coming from a key assignment or a hotkey.
    pub fn rgb_command(&mut self, code: u8) {
        // Effect cycling is invisible while music/screen frames own the LEDs.
        if matches!(code, 0 | 1) && self.cfg.profile().lighting.mode != LightingMode::Hardware {
            self.cfg.profile_mut().lighting.mode = LightingMode::Hardware;
            self.ui.audio_requested = false;
            self.ui.screen_requested = false;
        }
        let lighting = &mut self.cfg.profile_mut().lighting;
        match code {
            0 => lighting.effect = (lighting.effect + 1) % 13,
            1 => lighting.effect = (lighting.effect + 12) % 13,
            // The firmware takes five brightness/speed levels (0..=4 / 1..=5),
            // so a 10-point step often left the wire byte unchanged and the knob
            // felt dead for several clicks. One step per level always shows.
            2 => {
                let next = (lighting.brightness + 25).min(100);
                if next == lighting.brightness {
                    return;
                }
                lighting.brightness = next;
            }
            3 => {
                let next = lighting.brightness.saturating_sub(25);
                if next == lighting.brightness {
                    return;
                }
                lighting.brightness = next;
            }
            4 => {
                let next = (lighting.speed + 25).min(100);
                if next == lighting.speed {
                    return;
                }
                lighting.speed = next;
            }
            5 => {
                let next = lighting.speed.saturating_sub(25);
                if next == lighting.speed {
                    return;
                }
                lighting.speed = next;
            }
            6 => {
                if lighting.brightness > 0 {
                    lighting.brightness = 0;
                } else {
                    lighting.brightness = 80;
                }
            }
            7 => {
                let slot = (self.cfg.active_profile + 1) % self.cfg.profiles.len();
                self.switch_profile(slot);
                return;
            }
            _ => {}
        }
        self.ui.apply_lighting = true;
        self.ui.dirty = true;
        self.ui.notify("Lighting updated from keyboard command");
    }

    pub fn refresh_devices(&mut self) {
        self.devices = self.driver.discover();
        self.ui
            .notify(format!("Found {} HID interfaces", self.devices.len()));
    }

    pub fn load_action_into_editor(&mut self, index: usize) {
        let action = self.cfg.profile().action(self.ui.layer, index).clone();
        match &action {
            Action::None => self.ui.assign_kind = crate::ui::ActionKind::None,
            Action::Key { key } => {
                self.ui.assign_kind = crate::ui::ActionKind::Key;
                self.ui.assign_key = *key;
            }
            Action::Combo { mods, key } => {
                self.ui.assign_kind = crate::ui::ActionKind::Combo;
                self.ui.combo_mods = *mods;
                self.ui.combo_key = *key;
            }
            Action::Macro { macro_id } => {
                self.ui.assign_kind = crate::ui::ActionKind::Macro;
                self.ui.assign_macro = *macro_id;
            }
            Action::Media { media } => {
                self.ui.assign_kind = crate::ui::ActionKind::Media;
                self.ui.assign_media = *media;
            }
            Action::Fn => self.ui.assign_kind = crate::ui::ActionKind::Fn,
            Action::Rgb { command } => {
                self.ui.assign_kind = crate::ui::ActionKind::Rgb;
                self.ui.assign_rgb = *command;
            }
            Action::Profile { slot } => {
                self.ui.assign_kind = crate::ui::ActionKind::Profile;
                self.ui.assign_slot = *slot;
            }
        }
    }

    /// Macro being recorded right now, if any. The hook thread fills it.
    pub fn recording_id(&self) -> Option<u64> {
        self.hook.recorder.active_id()
    }

    pub fn recording_steps(&self) -> usize {
        self.hook.recorder.step_count()
    }

    pub fn recording_last_key(&self) -> Option<(u16, bool)> {
        self.hook.recorder.last_key()
    }

    pub fn start_recording(&mut self) {
        if self.recording_id().is_some() {
            self.ui.notify("A recording is already running");
            return;
        }
        let macros = &mut self.profile_mut().macros;
        let id = macros.iter().map(|m| m.id).max().unwrap_or(0) + 1;
        let name = format!("Recording {}", id);
        macros.push(crate::model::MacroDef::new(id, name));
        if !self.hook.recorder.start(id) {
            self.profile_mut().macros.retain(|m| m.id != id);
            self.ui.notify("A recording is already running");
            return;
        }
        self.ui.selected_macro = Some(id);
        self.ui.page = Page::Macros;
        self.ui
            .notify("Recording: press keys on the keyboard, then stop to save");
    }

    pub fn stop_recording(&mut self) {
        let Some(rec) = self.hook.recorder.finish() else {
            return;
        };
        let count = rec.steps.len();
        let recorded = rec.events;
        if let Some(def) = self.profile_mut().macro_by_id_mut(rec.macro_id) {
            def.steps = rec.steps;
            if def.name.starts_with("Recording") {
                def.name = format!("Macro {}", rec.macro_id);
            }
        }
        if count == 0 {
            self.profile_mut().macros.retain(|m| m.id != rec.macro_id);
            self.ui.selected_macro = None;
            self.ui.notify("Recording stopped: no key events captured");
        } else {
            self.ui.selected_macro = Some(rec.macro_id);
            self.ui.notify(format!(
                "Saved macro with {count} steps ({recorded} events)"
            ));
        }
        self.ui.dirty = true;
    }

    fn handle_event(&mut self, event: RawKeyEvent) {
        if event.vk == self.cfg.settings.fn_key_vk {
            self.ui.fn_held = event.down;
        }
        self.typed_key(event.vk, event.down);
    }

    /// Note a physical key transition for the keymap highlight and the tester.
    ///
    /// Both the Windows hook and the app's own key input feed this, so the
    /// transition that arrives second is dropped instead of counted twice.
    pub fn typed_key(&mut self, vk: u16, down: bool) {
        // Window input feeds the recorder too, so recording keeps working when
        // the hook cannot see the keyboard; the recorder drops the duplicate.
        self.hook.recorder.push(vk, down);
        match vk {
            0xAD | 0xB3 => self.ui.knob_press_at = Some(Instant::now()),
            0xAE | 0xAF => self.ui.knob_turn_at = Some(Instant::now()),
            _ => {}
        }
        let Some(index) = layout().index_of_vk(vk) else {
            return;
        };
        if down == self.ui.tester_down_at[index].is_some() {
            return;
        }
        let now = Instant::now();
        if down {
            self.activity.press(index);
            self.ui.tester_counts[index] = self.ui.tester_counts[index].saturating_add(1);
            self.ui.tester_total = self.ui.tester_total.saturating_add(1);
            self.ui.tester_down_at[index] = Some(now);
        } else {
            self.activity.release(index);
            if let Some(started) = self.ui.tester_down_at[index].take() {
                self.ui.tester_hold_ms[index] = started.elapsed().as_millis() as u32;
            }
            self.ui.tester_last[index] = Some(now);
        }
    }

    /// Apply one knob detent. Windows already handled the same consumer report
    /// as a volume step, so LED modes send the opposite step to cancel it.
    fn knob_turn(&mut self, up: bool) {
        self.ui.knob_turn_at = Some(Instant::now());
        let command = match self.cfg.settings.knob_turn {
            KnobTurn::Volume => return,
            KnobTurn::Brightness => {
                if up {
                    RgbCommand::BrightnessUp
                } else {
                    RgbCommand::BrightnessDown
                }
            }
            KnobTurn::Speed => {
                if up {
                    RgbCommand::SpeedUp
                } else {
                    RgbCommand::SpeedDown
                }
            }
            KnobTurn::Effects => {
                if up {
                    RgbCommand::NextEffect
                } else {
                    RgbCommand::PrevEffect
                }
            }
        };
        let before = {
            let lighting = &self.cfg.profile().lighting;
            (
                lighting.brightness,
                lighting.speed,
                lighting.effect,
                lighting.mode,
            )
        };
        self.rgb_command(crate::input::hook::rgb_command_code(command));
        let after = {
            let lighting = &self.cfg.profile().lighting;
            (
                lighting.brightness,
                lighting.speed,
                lighting.effect,
                lighting.mode,
            )
        };
        if before != after {
            // The board blanks for about half a second while it applies an LEDPARAM
            // write. Coalesce a burst of detents into the write that follows the
            // last one instead of stalling on every click.
            self.ui.apply_lighting = false;
            self.ui.lighting_edit_at = Some(Instant::now());
        }
        crate::input::inject::tap_key(if up { 0xAE } else { 0xAF });
    }

    fn handle_command(&mut self, command: HookCommand) {
        match command {
            HookCommand::SwitchProfile(slot) => self.switch_profile(slot as usize),
            HookCommand::RgbCommand(code) => self.rgb_command(code),
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let dt = now
            .duration_since(self.last_frame)
            .as_secs_f32()
            .clamp(0.0, 0.1);
        self.last_frame = now;

        while let Ok(event) = self.knob.events.try_recv() {
            match event {
                KnobEvent::Up => self.knob_turn(true),
                KnobEvent::Down => self.knob_turn(false),
                KnobEvent::Unavailable(message) => self
                    .ui
                    .notify(format!("Knob rotation unavailable: {message}")),
            }
        }
        while let Ok(event) = self.hook.events.try_recv() {
            self.handle_event(event);
        }
        while let Ok(command) = self.hook.commands.try_recv() {
            self.handle_command(command);
        }
        let tray_profiles = self
            .tray
            .as_ref()
            .map(|tray| tray.drain_profiles())
            .unwrap_or_default();
        for slot in tray_profiles {
            self.switch_profile(slot);
        }
        for status in self.player.drain_status() {
            match status {
                PlayerStatus::Started { name } => self.ui.notify(format!("Macro: {name}")),
                PlayerStatus::Finished { name } => {
                    self.ui.notify(format!("Macro finished: {name}"))
                }
                PlayerStatus::Stopped { name } => self.ui.notify(format!("Macro stopped: {name}")),
                PlayerStatus::Missing { id } => self.ui.notify(format!("Macro #{id} not found")),
            }
        }
        let restore_result = self.restore_rx.as_ref().map(|rx| rx.try_recv());
        if let Some(result) = restore_result {
            use std::sync::mpsc::TryRecvError;
            match result {
                Ok(Ok(0)) => {
                    self.restore_rx = None;
                    self.ui.restoring = false;
                    self.ui.notify("Onboard keymap already matches stock");
                }
                Ok(Ok(count)) => {
                    self.restore_rx = None;
                    self.ui.restoring = false;
                    self.ui
                        .notify(format!("Restored {count} stock keymap slot(s)"));
                }
                Ok(Err(err)) => {
                    self.restore_rx = None;
                    self.ui.restoring = false;
                    self.ui
                        .notify(format!("Onboard keymap restore failed: {err}"));
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.restore_rx = None;
                    self.ui.restoring = false;
                    self.ui
                        .notify("Onboard keymap restore thread stopped unexpectedly");
                }
            }
        }
        let upload_result = self.upload_rx.as_ref().map(|rx| rx.try_recv());
        if let Some(result) = upload_result {
            use std::sync::mpsc::TryRecvError;
            match result {
                Ok(Ok(count)) => {
                    self.upload_rx = None;
                    self.ui.lighting_uploading = false;
                    self.ui
                        .notify(format!("Per-key picture written ({count} packets)"));
                }
                Ok(Err(err)) => {
                    self.upload_rx = None;
                    self.ui.lighting_uploading = false;
                    self.ui.notify(format!("Per-key picture failed: {err}"));
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.upload_rx = None;
                    self.ui.lighting_uploading = false;
                    self.ui
                        .notify("Per-key picture worker stopped unexpectedly");
                }
            }
        }
        self.audio_snap = self.audio.snapshot();
        self.screen_snap = self.screen.snapshot();
        self.activity.tick(dt);

        // Entering a live output mode starts its capture once. An error clears
        // the request, and the mode has to change before it is retried.
        let lighting_mode = self.cfg.profile().lighting.mode;
        if self.last_lighting_mode != Some(lighting_mode) {
            self.last_lighting_mode = Some(lighting_mode);
            // Leaving a live output must stop its capture. Otherwise the old
            // music/screen stream keeps writing frames and hardware effects
            // look like they do nothing.
            self.ui.audio_requested = lighting_mode == LightingMode::Music;
            self.ui.screen_requested = lighting_mode == LightingMode::Screen;
        }

        // Audio capture lifecycle.
        let source = self.cfg.profile().lighting.music.source;
        if self.ui.audio_requested {
            if let Some(err) = self.audio_snap.error.clone() {
                self.audio.stop();
                self.ui.audio_requested = false;
                self.ui.notify(format!("Audio capture stopped: {err}"));
            } else if !self.audio.is_running() || self.last_audio_source != Some(source) {
                self.audio.start(source);
                self.last_audio_source = Some(source);
                self.ui.notify(format!("Audio capture: {}", source.label()));
            }
        } else if self.audio.is_running() {
            self.audio.stop();
            self.last_audio_source = None;
        }

        // Screen capture lifecycle (restart when region/grid/fps change).
        let screen_cfg = self.cfg.profile().lighting.screen.clone();
        if self.ui.screen_requested {
            if let Some(err) = self.screen_snap.error.clone() {
                self.screen.stop();
                self.ui.screen_requested = false;
                self.last_screen_cfg = None;
                self.ui.notify(format!("Screen capture stopped: {err}"));
            } else if !self.screen.is_running()
                || self.last_screen_cfg.as_ref() != Some(&screen_cfg)
            {
                self.screen.start(screen_cfg.clone());
                self.last_screen_cfg = Some(screen_cfg.clone());
                self.ui
                    .notify(format!("Screen capture at {} FPS", screen_cfg.fps));
            }
        } else if self.screen.is_running() {
            self.screen.stop();
            self.last_screen_cfg = None;
        }

        // Render the current lighting frame.
        {
            let live = LiveInputs {
                audio: &self.audio_snap,
                screen: &self.screen_snap,
            };
            let profile = self.cfg.profile();
            let colors = self
                .engine
                .render(&profile.lighting, layout(), dt, &live)
                .to_vec();
            self.colors = colors;
        }

        // Colour edits reach the keyboard as soon as the user stops dragging a
        // slider or picking a swatch, instead of waiting for an Apply button.
        if let Some(at) = self.ui.lighting_edit_at
            && now.duration_since(at) >= Duration::from_millis(120)
        {
            self.ui.lighting_edit_at = None;
            // Custom mode displays a flash-backed picture, so a mode switch on
            // every colour tweak would show the stored picture (or an empty
            // one) instead of the edit. The Send button writes both together.
            if !matches!(self.cfg.profile().lighting.mode, LightingMode::Custom) {
                self.ui.apply_lighting = true;
            }
        }

        // Driver writes requested by the UI or by a profile switch. Paused while
        // the picture worker owns the device, so two reports never interleave.
        if self.ui.apply_lighting && !self.ui.lighting_uploading {
            self.ui.apply_lighting = false;
            let lighting = self.cfg.profile().lighting.clone();
            match self.driver.apply_lighting(&lighting) {
                Ok(count) => self
                    .ui
                    .notify(format!("Lighting written ({count} packets)")),
                Err(err) => self.ui.notify(format!("Lighting write failed: {err}")),
            }
        }
        if self.ui.apply_keymap {
            self.ui.apply_keymap = false;
            if !self.onboard_keymap_active() {
                self.ui.notify(
                    "Onboard keymap write skipped: software remap is active (one remap path at a time)",
                );
            } else {
                let profile = self.cfg.profile();
                match self.driver.apply_keymap(
                    layout(),
                    &profile.normal,
                    &profile.func,
                    self.cfg.active_profile as u8,
                ) {
                    Ok(count) => self.ui.notify(format!("Keymap written ({count} packets)")),
                    Err(err) => self.ui.notify(format!("Keymap write failed: {err}")),
                }
            }
        }

        // Live modes go through the firmware's own host-fed paths: 0x0D band
        // heights (mode 20/22) and 0x0E screen colour (mode 21). Neither touches
        // flash, so they can run continuously.
        let mode = self.cfg.profile().lighting.mode;
        let interval =
            Duration::from_millis(1000 / self.cfg.settings.stream_fps.clamp(20, 60) as u64);
        if self.ui.stream_to_device
            && !self.ui.lighting_uploading
            && now.duration_since(self.last_stream_at) >= interval
        {
            self.last_stream_at = now;
            let result = match mode {
                LightingMode::Music => {
                    let heights = self.music_heights();
                    self.driver.stream_music_bands(&heights)
                }
                LightingMode::Screen => {
                    let rgb = self.screen_color();
                    self.driver.stream_screen_color(rgb)
                }
                _ => Ok(()),
            };
            match result {
                Ok(()) => self.last_stream_error = None,
                Err(err) => {
                    let text = err.to_string();
                    if self.last_stream_error.as_deref() != Some(text.as_str()) {
                        self.ui.notify(format!("RGB stream failed: {text}"));
                        self.last_stream_error = Some(text);
                    }
                }
            }
        }

        // Debounced save.
        if let Some(due) = self.save_due {
            if now >= due {
                self.save_due = None;
                let _ = self.cfg.save();
                self.saved_at = Some(now);
            }
        }

        let _ = self.driver.poll();

        // Close request: persist before the window goes away.
        // (Handled by the caller through `wants_close`.)
    }

    fn keymap_panel(&mut self, ui: &mut egui::Ui) {
        let profile_index = self.cfg.active_profile;
        let lighting_mode = self.cfg.profile().lighting.mode;
        let mut tool = self.ui.tool;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("LK67 65%").size(15.0).strong());
            theme::badge(ui, "68 KEYS", theme::ACCENT);
            ui.separator();
            if ui
                .selectable_label(self.ui.layer == Layer::Normal, "Normal")
                .clicked()
                && self.ui.layer != Layer::Normal
            {
                self.ui.layer = Layer::Normal;
                if let Some(index) = self.ui.selected {
                    self.load_action_into_editor(index);
                }
            }
            if ui
                .selectable_label(self.ui.layer == Layer::Fn, "FN layer")
                .clicked()
                && self.ui.layer != Layer::Fn
            {
                self.ui.layer = Layer::Fn;
                if let Some(index) = self.ui.selected {
                    self.load_action_into_editor(index);
                }
            }
            ui.separator();
            tool = self.ui.tool;
            ui.selectable_value(&mut self.ui.tool, Tool::Select, "Select");
            ui.selectable_value(&mut self.ui.tool, Tool::Paint, "Paint");
            if tool != self.ui.tool {
                let message = match self.ui.tool {
                    Tool::Paint => "Paint mode: click or drag keys to colour them",
                    Tool::Select => "Select mode",
                };
                self.ui.notify(message);
            }
            ui.separator();
            ui.checkbox(&mut self.cfg.settings.show_keymap_rgb, "RGB preview");
            if self.ui.fn_held {
                theme::badge(ui, "FN HELD", theme::WARN);
            }
            ui.label(
                egui::RichText::new(format!("lighting: {}", lighting_mode.short()))
                    .size(11.0)
                    .color(theme::MUTED),
            );
        });
        if self.ui.tool == Tool::Paint {
            theme::hint(
                ui,
                "Left click / drag paints the selected colour, right click clears.",
            );
        }

        let show_rgb = self.cfg.settings.show_keymap_rgb;
        let heat = if self.ui.page == Page::Tester && self.ui.tester_heatmap {
            Some(self.ui.tester_counts.as_slice())
        } else {
            None
        };
        let response = {
            let profile = &self.cfg.profiles[profile_index];
            let view = KeymapView {
                layout: layout(),
                layer: self.ui.layer,
                normal: &profile.normal,
                func: &profile.func,
                macros: &profile.macros,
                colors: &self.colors,
                show_rgb,
                pressed: &self.activity.pressed,
                selected: self.ui.selected,
                heat,
                tool: self.ui.tool,
            };
            keymap_show(ui, &view)
        };

        {
            if self.ui.tool == Tool::Paint {
                let color = self.ui.paint_color;
                let profile = &mut self.cfg.profiles[profile_index];
                for index in &response.painted {
                    if let Some(slot) = profile.lighting.custom.keys.get_mut(*index) {
                        *slot = color;
                    }
                }
                if let Some(index) = response.right_clicked {
                    if let Some(slot) = profile.lighting.custom.keys.get_mut(index) {
                        *slot = [0, 0, 0];
                    }
                }
                if !response.painted.is_empty() || response.right_clicked.is_some() {
                    profile.lighting.mode = LightingMode::Custom;
                    // A playing animation would override the painted layout in
                    // the preview and in every frame, so painting pins the
                    // static picture instead.
                    profile.lighting.animation_playing = false;
                    self.ui.dirty = true;
                    self.ui.lighting_edit_at = None;
                }
                // Painting also selects the key so it can be remapped straight
                // after clicking it, instead of having to switch tools.
                if let Some(index) = response.clicked {
                    self.ui.selected = Some(index);
                    self.load_action_into_editor(index);
                }
            } else if let Some(index) = response.clicked {
                self.ui.selected = Some(index);
                self.load_action_into_editor(index);
                self.ui
                    .notify(format!("Selected {}", layout().label(index)));
            }
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let mut switch = None;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("LK67")
                    .size(20.0)
                    .strong()
                    .color(theme::ACCENT),
            );
            ui.label(egui::RichText::new("Studio").size(20.0).strong());
            ui.add_space(4.0);
            theme::badge(ui, "65% RGB", theme::PANEL_3);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (color, label) = match self.driver.status() {
                    DeviceStatus::Connected { .. } => (theme::OK, "CONNECTED"),
                    DeviceStatus::Simulation => (theme::WARN, "DRY-RUN"),
                    DeviceStatus::Disconnected => (theme::DANGER, "OFFLINE"),
                };
                theme::badge(ui, label, color);
                if self.ui.dirty || self.save_due.is_some() {
                    theme::badge(ui, "UNSAVED", theme::WARN);
                } else if self.saved_at.is_some() {
                    theme::badge(ui, "SAVED", theme::ACCENT_DIM);
                }
            });
        });
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("PROFILE")
                    .size(10.0)
                    .strong()
                    .color(theme::MUTED),
            );
            for slot in 0..self.cfg.profiles.len() {
                let selected = self.cfg.active_profile == slot;
                let name = widgets::truncate(&self.cfg.profiles[slot].name, 16);
                let text = format!("{}  {}", slot + 1, name);
                if ui.selectable_label(selected, text).clicked() && !selected {
                    switch = Some(slot);
                }
            }
            ui.separator();
            let mut dry = self.cfg.settings.dry_run;
            if ui.checkbox(&mut dry, "Dry-run").changed() {
                self.cfg.settings.dry_run = dry;
                self.driver.set_dry_run(dry);
                self.ui.notify(if dry {
                    "Dry-run enabled: packets are logged, never written"
                } else {
                    "Hardware mode: HID writes enabled"
                });
                self.ui.dirty = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Save now").clicked() {
                    self.save_now();
                }
            });
        });
        if let Some(slot) = switch {
            self.switch_profile(slot);
        }
    }

    fn nav(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("WORKSPACE")
                .size(10.0)
                .strong()
                .color(theme::MUTED),
        );
        for page in Page::ALL {
            let selected = self.ui.page == page;
            let text = egui::RichText::new(page.label()).size(13.0);
            let response = ui.selectable_label(selected, text);
            if response.clicked() {
                self.ui.page = page;
                if page == Page::Driver && self.devices.is_empty() {
                    self.refresh_devices();
                }
            }
            response.on_hover_text(page.hint());
        }
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("CONNECTION")
                .size(10.0)
                .strong()
                .color(theme::MUTED),
        );
        theme::card(ui, "", |ui| {
            let status = self.driver.status().clone();
            match &status {
                DeviceStatus::Connected {
                    product,
                    manufacturer,
                    ..
                } => {
                    ui.label(
                        egui::RichText::new(if product.is_empty() { "LK67" } else { product })
                            .size(13.0)
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new(manufacturer)
                            .size(10.5)
                            .color(theme::MUTED),
                    );
                }
                DeviceStatus::Simulation => {
                    ui.label(
                        egui::RichText::new("Simulation mode")
                            .size(13.0)
                            .strong()
                            .color(theme::WARN),
                    );
                    ui.label(
                        egui::RichText::new("Packets are validated + logged")
                            .size(10.5)
                            .color(theme::MUTED),
                    );
                }
                DeviceStatus::Disconnected => {
                    ui.label(
                        egui::RichText::new("No device")
                            .size(13.0)
                            .strong()
                            .color(theme::DANGER),
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "Looking for {:04X}:{:04X}",
                            self.cfg.driver.vid, self.cfg.driver.pid
                        ))
                        .size(10.5)
                        .color(theme::MUTED),
                    );
                }
            }
            ui.add_space(4.0);
            if let Some(err) = self.driver.last_error() {
                theme::hint(ui, &widgets::truncate(err, 60));
            }
        });
        ui.add_space(8.0);
        ui.checkbox(&mut self.ui.stream_to_device, "Stream colours to keys");
        theme::hint(
            ui,
            "Needed for custom, music and screen modes on real hardware.",
        );

        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            ui.label(
                egui::RichText::new("v0.1.0  placeholder protocol")
                    .size(9.5)
                    .color(theme::MUTED),
            );
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let (color, text) = match self.driver.status() {
                DeviceStatus::Connected { .. } => (theme::OK, "LIVE"),
                DeviceStatus::Simulation => (theme::WARN, "DRY-RUN"),
                DeviceStatus::Disconnected => (theme::DANGER, "OFFLINE"),
            };
            theme::badge(ui, text, color);
            ui.label(
                egui::RichText::new(self.driver.status().label())
                    .size(11.0)
                    .color(theme::MUTED),
            );
            ui.separator();
            ui.label(
                egui::RichText::new(format!(
                    "P{} - {}",
                    self.cfg.active_profile + 1,
                    self.cfg.profile().name
                ))
                .size(11.0)
                .color(theme::MUTED),
            );
            ui.separator();
            ui.label(
                egui::RichText::new(self.cfg.profile().lighting.mode.label())
                    .size(11.0)
                    .color(theme::MUTED),
            );
            if self.recording_id().is_some() {
                ui.separator();
                theme::badge(ui, "REC", theme::DANGER);
                ui.label(
                    egui::RichText::new(format!("{} steps", self.recording_steps()))
                        .size(11.0)
                        .color(theme::DANGER),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let status = &self.ui.status;
                let age = self
                    .ui
                    .status_at
                    .map(|t| t.elapsed().as_secs_f32())
                    .unwrap_or(99.0);
                let color = if age < 12.0 {
                    theme::TEXT
                } else {
                    theme::MUTED
                };
                ui.label(egui::RichText::new(status).size(11.0).color(color));
            });
        });
    }

    fn central(&mut self, ui: &mut egui::Ui) {
        self.keymap_panel(ui);
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .id_salt("page-scroll")
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                match self.ui.page {
                    Page::Remap => crate::ui::remap::draw(self, ui),
                    Page::Lighting => crate::ui::lighting::draw(self, ui),
                    Page::Macros => crate::ui::macros::draw(self, ui),
                    Page::Tester => crate::ui::tester::draw(self, ui),
                    Page::Music => crate::ui::music::draw(self, ui),
                    Page::Screen => crate::ui::screen::draw(self, ui),
                    Page::Driver => crate::ui::driver::draw(self, ui),
                    Page::Settings => crate::ui::settings::draw(self, ui),
                }
                ui.add_space(12.0);
            });
    }
}

/// Map an egui key to the Windows virtual-key code the layout uses.
fn egui_key_vk(key: egui::Key) -> Option<u16> {
    use egui::Key;
    Some(match key {
        Key::A => 0x41,
        Key::B => 0x42,
        Key::C => 0x43,
        Key::D => 0x44,
        Key::E => 0x45,
        Key::F => 0x46,
        Key::G => 0x47,
        Key::H => 0x48,
        Key::I => 0x49,
        Key::J => 0x4A,
        Key::K => 0x4B,
        Key::L => 0x4C,
        Key::M => 0x4D,
        Key::N => 0x4E,
        Key::O => 0x4F,
        Key::P => 0x50,
        Key::Q => 0x51,
        Key::R => 0x52,
        Key::S => 0x53,
        Key::T => 0x54,
        Key::U => 0x55,
        Key::V => 0x56,
        Key::W => 0x57,
        Key::X => 0x58,
        Key::Y => 0x59,
        Key::Z => 0x5A,
        Key::Num0 => 0x30,
        Key::Num1 => 0x31,
        Key::Num2 => 0x32,
        Key::Num3 => 0x33,
        Key::Num4 => 0x34,
        Key::Num5 => 0x35,
        Key::Num6 => 0x36,
        Key::Num7 => 0x37,
        Key::Num8 => 0x38,
        Key::Num9 => 0x39,
        Key::Escape => 0x1B,
        Key::Tab => 0x09,
        Key::Backspace => 0x08,
        Key::Enter => 0x0D,
        Key::Space => 0x20,
        Key::Insert => 0x2D,
        Key::Delete => 0x2E,
        Key::Home => 0x24,
        Key::End => 0x23,
        Key::PageUp => 0x21,
        Key::PageDown => 0x22,
        Key::ArrowUp => 0x26,
        Key::ArrowDown => 0x28,
        Key::ArrowLeft => 0x25,
        Key::ArrowRight => 0x27,
        Key::Minus => 0xBD,
        Key::Comma => 0xBC,
        Key::Period => 0xBE,
        Key::Slash => 0xBF,
        Key::Semicolon => 0xBA,
        Key::Quote => 0xDE,
        Key::Backtick => 0xC0,
        Key::OpenBracket => 0xDB,
        Key::CloseBracket => 0xDD,
        Key::Backslash => 0xDC,
        _ => return None,
    })
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.tick();
        // Real key presses seen by this window also feed the tester, so it works
        // even when the Windows hook could not be installed.
        let typed: Vec<(u16, bool)> = ui.input(|input| {
            input
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Key { key, pressed, .. } => {
                        egui_key_vk(*key).map(|vk| (vk, *pressed))
                    }
                    _ => None,
                })
                .collect()
        });
        for (vk, down) in typed {
            self.typed_key(vk, down);
        }

        egui::Panel::top("top-bar")
            .frame(
                egui::Frame::NONE
                    .fill(theme::PANEL)
                    .inner_margin(egui::Margin {
                        left: 14,
                        right: 14,
                        top: 8,
                        bottom: 8,
                    })
                    .stroke(egui::Stroke::new(1.0, theme::STROKE)),
            )
            .show(ui, |ui| self.top_bar(ui));

        egui::Panel::bottom("status-bar")
            .exact_size(26.0)
            .frame(
                egui::Frame::NONE
                    .fill(theme::PANEL)
                    .inner_margin(egui::Margin {
                        left: 12,
                        right: 12,
                        top: 4,
                        bottom: 4,
                    }),
            )
            .show(ui, |ui| self.status_bar(ui));

        egui::Panel::left("nav")
            .exact_size(208.0)
            .resizable(false)
            .frame(
                egui::Frame::NONE
                    .fill(theme::PANEL)
                    .inner_margin(egui::Margin {
                        left: 10,
                        right: 10,
                        top: 2,
                        bottom: 6,
                    }),
            )
            .show(ui, |ui| self.nav(ui));

        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(theme::BG)
                    .inner_margin(egui::Margin {
                        left: 16,
                        right: 16,
                        top: 12,
                        bottom: 12,
                    }),
            )
            .show(ui, |ui| self.central(ui));

        // Push UI edits into the hook, player and save schedule.
        if self.ui.dirty {
            self.ui.dirty = false;
            self.player.sync_macros(&self.cfg.profile().macros);
            self.hook
                .set_config(HookConfig::build(&self.cfg.settings, self.cfg.profile()));
            self.save_due = Some(Instant::now() + Duration::from_millis(700));
        }

        if ui.ctx().input(|i| i.viewport().close_requested()) {
            let _ = self.cfg.save();
            // A tray icon makes closing hide the window instead of quitting;
            // Quit lives in the tray menu (it sets `quitting` first).
            if self.tray.is_some() && !self.quitting.load(Ordering::Relaxed) {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
        }

        let live = self.ui.audio_requested
            || self.ui.screen_requested
            || self.cfg.profile().lighting.animation_playing
            || self.activity.any_pressed();
        ui.ctx()
            .request_repaint_after(Duration::from_millis(if live { 16 } else { 40 }));
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = self.cfg.save();
    }
}
