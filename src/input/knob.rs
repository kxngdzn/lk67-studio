//! Rotary-knob input.
//!
//! The knob is not a keyboard key. Its detents arrive as HID consumer-control
//! reports (`0xE9` / `0xEA`) on the keyboard's consumer collection; Windows turns
//! those into the volume overlay without ever producing a `WH_KEYBOARD_LL` event,
//! so the hook cannot see them. This reader opens that collection directly and
//! forwards the two usages the app cares about.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use hidapi::HidApi;

#[derive(Clone, Debug)]
pub enum KnobEvent {
    Up,
    Down,
    /// The consumer collection could not be read. Sent once per failure reason.
    Unavailable(String),
}

pub struct KnobWatcher {
    pub events: Receiver<KnobEvent>,
    running: Arc<AtomicBool>,
}

impl Drop for KnobWatcher {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

/// Start the reader. It rescans every two seconds, so unplugging and replugging
/// the keyboard does not require an app restart.
pub fn start(vid: u16, pid: u16) -> KnobWatcher {
    let (tx, events) = mpsc::channel();
    let running = Arc::new(AtomicBool::new(true));
    let flag = running.clone();
    let _ = std::thread::Builder::new()
        .name("lk67-knob".into())
        .spawn(move || watch(vid, pid, tx, flag));
    KnobWatcher { events, running }
}

fn watch(vid: u16, pid: u16, tx: Sender<KnobEvent>, running: Arc<AtomicBool>) {
    let api = match HidApi::new() {
        Ok(api) => api,
        Err(err) => {
            let _ = tx.send(KnobEvent::Unavailable(format!("HID init failed: {err}")));
            return;
        }
    };
    let mut reported = String::new();
    while running.load(Ordering::Relaxed) {
        let path = api
            .device_list()
            .find(|info| {
                info.vendor_id() == vid && info.product_id() == pid && info.usage_page() == 0x0C
            })
            .map(|info| info.path().to_owned());
        let Some(path) = path else {
            note(&tx, &mut reported, "no consumer control collection");
            std::thread::sleep(Duration::from_secs(2));
            continue;
        };
        let device = match api.open_path(path.as_c_str()) {
            Ok(device) => device,
            Err(err) => {
                note(
                    &tx,
                    &mut reported,
                    &format!("consumer collection open failed: {err}"),
                );
                std::thread::sleep(Duration::from_secs(2));
                continue;
            }
        };
        let _ = device.set_blocking_mode(true);
        crate::input::hook::debug_note("knob consumer collection open");
        reported.clear();
        let mut buf = [0u8; 64];
        let mut held: Option<u16> = None;
        loop {
            if !running.load(Ordering::Relaxed) {
                return;
            }
            match device.read_timeout(&mut buf, 250) {
                Ok(0) => {}
                Ok(n) => {
                    let usage = decode_usage(&buf[..n]);
                    match usage {
                        Some(0xE9) | Some(0xEA) => {
                            // One press report per detent; the following zero
                            // report releases it. Ignore a repeated press.
                            if held != usage {
                                held = usage;
                                let up = usage == Some(0xE9);
                                let _ = tx.send(if up { KnobEvent::Up } else { KnobEvent::Down });
                                crate::input::hook::debug_note(if up {
                                    "knob turn up"
                                } else {
                                    "knob turn down"
                                });
                            }
                        }
                        _ => held = None,
                    }
                }
                Err(err) => {
                    let message = format!("consumer collection read failed: {err}");
                    crate::input::hook::debug_note(&message);
                    note(&tx, &mut reported, &message);
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn note(tx: &Sender<KnobEvent>, reported: &mut String, message: &str) {
    if reported == message {
        return;
    }
    *reported = message.to_string();
    let _ = tx.send(KnobEvent::Unavailable(message.to_string()));
}

/// Consumer reports carry the usage little-endian after the report ID:
/// `03 E9 00` is volume up, `03 EA 00` volume down, `03 00 00` release.
/// A two-byte report has no report ID and starts with the usage instead.
fn decode_usage(report: &[u8]) -> Option<u16> {
    match report.len() {
        0 | 1 => None,
        2 => Some(u16::from_le_bytes([report[0], report[1]])),
        _ => Some(u16::from_le_bytes([report[1], report[2]])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_knob_usages() {
        assert_eq!(decode_usage(&[0x03, 0xE9, 0x00]), Some(0xE9));
        assert_eq!(decode_usage(&[0x03, 0xEA, 0x00]), Some(0xEA));
        assert_eq!(decode_usage(&[0x03, 0x00, 0x00]), Some(0));
        assert_eq!(decode_usage(&[0xE9, 0x00]), Some(0xE9));
        assert_eq!(decode_usage(&[]), None);
        assert_eq!(decode_usage(&[0x03]), None);
    }
}
