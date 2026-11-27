//! Software remapping, macro playback and the real-time key tester.
//!
//! The low-level Windows hook reports every physical key event and, when software
//! remapping is enabled, swallows/intercepts keys and injects the mapped output.
//! Firmware keymaps remain the primary mechanism; this layer is what makes the app
//! usable in Dry-Run and on boards without writable keymaps.

pub mod hook;
pub mod inject;
pub mod macro_player;
pub mod recorder;

pub use hook::{HookCommand, HookConfig, HookHandle, RawKeyEvent, start_hook};
pub use macro_player::{MacroPlayer, PlayerStatus};
