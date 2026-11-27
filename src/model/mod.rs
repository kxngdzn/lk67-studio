//! Pure data model: key codes, the 65% layout, actions, macros, lighting and persistence.
//!
//! Nothing in here talks to hardware or to the UI toolkit, which keeps the model easy
//! to test, serialize and share with the (future) firmware implementation.

pub mod action;
pub mod keycode;
pub mod layout;
pub mod lighting;
pub mod macro_def;
pub mod profile;
pub mod store;

pub use action::Action;
pub use macro_def::MacroDef;
pub use profile::Profile;
pub use store::Config;

/// The two remappable layers of the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Layer {
    #[default]
    Normal,
    Fn,
}

impl Layer {
    pub fn label(self) -> &'static str {
        match self {
            Layer::Normal => "Normal",
            Layer::Fn => "FN",
        }
    }
}
