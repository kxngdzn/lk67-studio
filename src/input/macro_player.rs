//! Macro playback on a dedicated thread so timing never depends on the UI frame rate.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use crate::input::inject::{send_key, send_mods, tap_combo};
use crate::model::macro_def::{MacroDef, MacroStep, RepeatMode};

#[derive(Clone, Copy, Debug)]
pub enum PlayerJob {
    /// A key bound to a macro was pressed.
    Trigger {
        id: u64,
    },
    /// The key bound to a hold macro was released.
    Release {
        id: u64,
    },
    /// One-off playback from the UI.
    PlayOnce {
        id: u64,
    },
    StopAll,
    #[allow(dead_code)]
    Shutdown,
}

#[derive(Clone, Debug)]
pub enum PlayerStatus {
    Started { name: String },
    Finished { name: String },
    Stopped { name: String },
    Missing { id: u64 },
}

pub struct MacroPlayer {
    tx: Sender<PlayerJob>,
    status: Receiver<PlayerStatus>,
    registry: Arc<RwLock<HashMap<u64, MacroDef>>>,
}

impl MacroPlayer {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<PlayerJob>();
        let (status_tx, status_rx) = mpsc::channel::<PlayerStatus>();
        let registry: Arc<RwLock<HashMap<u64, MacroDef>>> = Arc::new(RwLock::new(HashMap::new()));
        let thread_registry = registry.clone();
        thread::Builder::new()
            .name("lk67-macros".into())
            .spawn(move || player_thread(rx, status_tx, thread_registry))
            .expect("spawn macro player");
        MacroPlayer {
            tx,
            status: status_rx,
            registry,
        }
    }

    pub fn sender(&self) -> Sender<PlayerJob> {
        self.tx.clone()
    }

    /// Keep the player's view of the active profile's macros up to date.
    pub fn sync_macros(&self, macros: &[MacroDef]) {
        if let Ok(mut guard) = self.registry.write() {
            guard.clear();
            for def in macros {
                guard.insert(def.id, def.clone());
            }
        }
    }

    pub fn play_once(&self, id: u64) {
        let _ = self.tx.send(PlayerJob::PlayOnce { id });
    }

    pub fn stop_all(&self) {
        let _ = self.tx.send(PlayerJob::StopAll);
    }

    pub fn drain_status(&self) -> Vec<PlayerStatus> {
        let mut out = Vec::new();
        while let Ok(status) = self.status.try_recv() {
            out.push(status);
        }
        out
    }
}

impl Default for MacroPlayer {
    fn default() -> Self {
        Self::new()
    }
}

struct Active {
    token: Arc<AtomicBool>,
    name: String,
}

fn player_thread(
    rx: Receiver<PlayerJob>,
    status: Sender<PlayerStatus>,
    registry: Arc<RwLock<HashMap<u64, MacroDef>>>,
) {
    let active: Arc<Mutex<HashMap<u64, Active>>> = Arc::new(Mutex::new(HashMap::new()));
    while let Ok(job) = rx.recv() {
        match job {
            PlayerJob::Trigger { id } => {
                let def = registry.read().ok().and_then(|g| g.get(&id).cloned());
                let Some(def) = def else {
                    let _ = status.send(PlayerStatus::Missing { id });
                    continue;
                };
                if def.is_empty() {
                    continue;
                }
                match def.repeat {
                    RepeatMode::Toggle | RepeatMode::Hold => {
                        let already = active.lock().map(|g| g.contains_key(&id)).unwrap_or(false);
                        if already {
                            stop(&active, id, &status);
                        } else {
                            let token = Arc::new(AtomicBool::new(false));
                            if let Ok(mut guard) = active.lock() {
                                guard.insert(
                                    id,
                                    Active {
                                        token: token.clone(),
                                        name: def.name.clone(),
                                    },
                                );
                            }
                            spawn_loop(def, 0, token, status.clone(), None);
                        }
                    }
                    RepeatMode::Once => spawn_loop(
                        def,
                        1,
                        Arc::new(AtomicBool::new(false)),
                        status.clone(),
                        None,
                    ),
                    RepeatMode::Count { count } => spawn_loop(
                        def,
                        count.max(1),
                        Arc::new(AtomicBool::new(false)),
                        status.clone(),
                        None,
                    ),
                }
            }
            PlayerJob::Release { id } => {
                stop(&active, id, &status);
            }
            PlayerJob::PlayOnce { id } => {
                let def = registry.read().ok().and_then(|g| g.get(&id).cloned());
                if let Some(def) = def {
                    if !def.is_empty() {
                        spawn_loop(
                            def,
                            1,
                            Arc::new(AtomicBool::new(false)),
                            status.clone(),
                            None,
                        );
                    }
                } else {
                    let _ = status.send(PlayerStatus::Missing { id });
                }
            }
            PlayerJob::StopAll => {
                if let Ok(mut guard) = active.lock() {
                    for (_, job) in guard.drain() {
                        job.token.store(true, Ordering::Relaxed);
                        let _ = status.send(PlayerStatus::Stopped { name: job.name });
                    }
                }
            }
            PlayerJob::Shutdown => break,
        }
    }
}

fn stop(active: &Arc<Mutex<HashMap<u64, Active>>>, id: u64, status: &Sender<PlayerStatus>) {
    if let Ok(mut guard) = active.lock() {
        if let Some(job) = guard.remove(&id) {
            job.token.store(true, Ordering::Relaxed);
            let _ = status.send(PlayerStatus::Stopped { name: job.name });
        }
    }
}

fn spawn_loop(
    def: MacroDef,
    repeat: u32,
    token: Arc<AtomicBool>,
    status: Sender<PlayerStatus>,
    cleanup: Option<u64>,
) {
    let _ = status.send(PlayerStatus::Started {
        name: def.name.clone(),
    });
    thread::Builder::new()
        .name("lk67-macro-play".into())
        .spawn(move || {
            let endless = repeat == 0;
            let mut count = 0u32;
            loop {
                if token.load(Ordering::Relaxed) {
                    break;
                }
                run_once(&def, &token);
                count += 1;
                if !endless && count >= repeat {
                    break;
                }
                if endless && !sleep_cancelled(30, &token) {
                    break;
                }
            }
            if let Some(id) = cleanup {
                let _ = id;
            }
            let _ = status.send(PlayerStatus::Finished {
                name: def.name.clone(),
            });
        })
        .ok();
}

fn run_once(def: &MacroDef, token: &Arc<AtomicBool>) {
    let started = Instant::now();
    for step in &def.steps {
        if token.load(Ordering::Relaxed) || started.elapsed() > Duration::from_secs(120) {
            break;
        }
        let delay = if step.delay() > 0 {
            step.delay()
        } else {
            def.default_delay_ms
        };
        match step {
            MacroStep::Down { key, .. } => {
                send_key(key.vk(), true);
                if !sleep_cancelled(delay, token) {
                    return;
                }
            }
            MacroStep::Up { key, .. } => {
                send_key(key.vk(), false);
                if !sleep_cancelled(delay, token) {
                    return;
                }
            }
            MacroStep::Tap { key, .. } => {
                send_key(key.vk(), true);
                if !sleep_cancelled(8, token) {
                    return;
                }
                send_key(key.vk(), false);
                if !sleep_cancelled(delay, token) {
                    return;
                }
            }
            MacroStep::Combo {
                mods, key, hold_ms, ..
            } => {
                send_mods(mods.bits(), true);
                send_key(key.vk(), true);
                if !sleep_cancelled((*hold_ms).max(8), token) {
                    send_key(key.vk(), false);
                    send_mods(mods.bits(), false);
                    return;
                }
                send_key(key.vk(), false);
                send_mods(mods.bits(), false);
                if !sleep_cancelled(delay, token) {
                    return;
                }
            }
            MacroStep::Delay { .. } => {
                if !sleep_cancelled(delay, token) {
                    return;
                }
            }
        }
    }
    // Release anything a malformed macro left held.
    for step in &def.steps {
        if let MacroStep::Down { key, .. } = step {
            send_key(key.vk(), false);
        }
    }
}

/// Sleep in small slices so a stop request is honoured quickly. Returns false when
/// playback was cancelled.
fn sleep_cancelled(ms: u32, token: &Arc<AtomicBool>) -> bool {
    let mut remaining = ms.min(60_000);
    while remaining > 0 {
        if token.load(Ordering::Relaxed) {
            return false;
        }
        let slice = remaining.min(4);
        thread::sleep(Duration::from_millis(slice as u64));
        remaining -= slice;
    }
    !token.load(Ordering::Relaxed)
}

#[allow(dead_code)]
fn tap(vk: u16, bits: u8) {
    if bits == 0 {
        crate::input::inject::tap_key(vk);
    } else {
        tap_combo(bits, vk);
    }
}
