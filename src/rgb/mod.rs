//! Software lighting engine.
//!
//! Every mode produces one RGB value per key per frame:
//! * the 13 hardware effects are previewed here so the UI can show them even in dry-run,
//! * custom palettes / keyframe animations are computed locally and streamed as per-key
//!   colours,
//! * music and screen-follow modes turn live input into per-key colours.

pub mod effects;
pub mod music;
pub mod screen;

pub use effects::{KeyActivity, LiveInputs, RgbEngine};
pub use music::{AudioEngine, AudioSnapshot};
pub use screen::{ScreenEngine, ScreenSnapshot};
