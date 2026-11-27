//! Macro recording that lives on the hook path.
//!
//! The low-level hook appends events here directly instead of handing them to the
//! UI first, so a recording keeps every keystroke even while the window is hidden,
//! unfocused or simply waiting for its next repaint. The UI only reads the current
//! step count and takes the finished steps when the user stops.

use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

use crate::model::keycode::KeyCode;
use crate::model::macro_def::MacroStep;

#[derive(Clone, Debug)]
pub struct RecordingSnapshot {
    pub macro_id: u64,
    pub steps: Vec<MacroStep>,
    pub events: u32,
}

struct Session {
    macro_id: u64,
    last_at: Instant,
    steps: Vec<MacroStep>,
    held: Vec<KeyCode>,
    events: u32,
    /// Current physical state per virtual key, so the hook and the app's own
    /// input can both report a key without recording it twice.
    down: [bool; 256],
    /// Most recent captured key, shown next to the recording badge.
    last: Option<(u16, bool)>,
}

/// Shared recorder slot: at most one recording at a time.
#[derive(Default)]
pub struct Recorder {
    session: Mutex<Option<Session>>,
    /// Woken on every recorded key so the step counter stays live even when
    /// another window has focus.
    repaint: Mutex<Option<egui::Context>>,
}

impl Recorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Let the recorder ask the UI for a repaint after each captured key.
    pub fn set_repaint_context(&self, ctx: egui::Context) {
        let mut guard = self
            .repaint
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = Some(ctx);
    }

    /// Begin a recording for `macro_id`. Returns false when one is already running.
    pub fn start(&self, macro_id: u64) -> bool {
        let mut guard = self.lock();
        if guard.is_some() {
            return false;
        }
        *guard = Some(Session {
            macro_id,
            last_at: Instant::now(),
            steps: Vec::new(),
            held: Vec::new(),
            events: 0,
            down: [false; 256],
            last: None,
        });
        debug_recorder(&format!("record start macro={macro_id}"));
        true
    }

    /// End the recording and hand the captured steps to the caller.
    pub fn finish(&self) -> Option<RecordingSnapshot> {
        self.lock().take().map(|session| {
            debug_recorder(&format!(
                "record stop steps={} events={}",
                session.steps.len(),
                session.events
            ));
            RecordingSnapshot {
                macro_id: session.macro_id,
                steps: session.steps,
                events: session.events,
            }
        })
    }

    pub fn active_id(&self) -> Option<u64> {
        self.lock().as_ref().map(|session| session.macro_id)
    }

    pub fn step_count(&self) -> usize {
        self.lock()
            .as_ref()
            .map(|session| session.steps.len())
            .unwrap_or(0)
    }

    /// The key captured most recently by the active recording.
    pub fn last_key(&self) -> Option<(u16, bool)> {
        self.lock().as_ref().and_then(|session| session.last)
    }

    /// Called for every physical key event, from the hook thread and from the
    /// app's own window input. Repeats and the second report of the same
    /// transition are ignored.
    pub fn push(&self, vk: u16, down: bool) {
        {
            let mut guard = self.lock();
            let Some(session) = guard.as_mut() else {
                return;
            };
            let slot = (vk as usize).min(255);
            if session.down[slot] == down {
                return;
            }
            session.down[slot] = down;
            session.last = Some((vk, down));
            debug_recorder(&format!("record step vk={vk:02X} down={}", down as u8));
            let now = Instant::now();
            let key = KeyCode(vk);
            let delay = now.duration_since(session.last_at).as_millis().min(2000) as u32;
            if down {
                if !session.held.contains(&key) {
                    session.held.push(key);
                }
                session.steps.push(MacroStep::Down {
                    key,
                    delay_ms: delay,
                });
            } else {
                session.held.retain(|held| *held != key);
                session.steps.push(MacroStep::Up {
                    key,
                    delay_ms: delay,
                });
            }
            session.last_at = now;
            session.events += 1;
        }
        let ctx = self
            .repaint
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if let Some(ctx) = ctx {
            ctx.request_repaint();
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<Session>> {
        self.session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Recording trace in `%APPDATA%\LK67 Studio\hook.log` when `LK67_DEBUG_KEYS` is set.
fn debug_recorder(message: &str) {
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
        let _ = writeln!(file, "-- {message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_press_and_release() {
        let recorder = Recorder::new();
        assert!(recorder.start(7));
        assert!(!recorder.start(9), "second recording must be refused");
        recorder.push(0x41, true);
        recorder.push(0x41, false);
        assert_eq!(recorder.step_count(), 2);

        let snapshot = recorder.finish().expect("session");
        assert_eq!(snapshot.macro_id, 7);
        assert_eq!(snapshot.events, 2);
        assert!(matches!(
            snapshot.steps.first(),
            Some(MacroStep::Down { key, .. }) if key.vk() == 0x41
        ));
        assert!(
            matches!(snapshot.steps.get(1), Some(MacroStep::Up { key, .. }) if key.vk() == 0x41)
        );

        assert!(recorder.start(8));
        assert_eq!(recorder.active_id(), Some(8));
    }

    #[test]
    fn duplicate_transitions_are_ignored() {
        let recorder = Recorder::new();
        assert!(recorder.start(3));
        recorder.push(0x41, true);
        recorder.push(0x41, true);
        recorder.push(0x41, false);
        recorder.push(0x41, false);
        assert_eq!(recorder.step_count(), 2);
        let snapshot = recorder.finish().expect("session");
        assert_eq!(snapshot.events, 2);
    }

    #[test]
    fn ignores_events_without_a_session() {
        let recorder = Recorder::new();
        recorder.push(0x41, true);
        assert_eq!(recorder.step_count(), 0);
        assert!(recorder.finish().is_none());
    }
}
