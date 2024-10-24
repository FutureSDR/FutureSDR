//! Blocks that have corresponding GUI widgets.
//!
//! These can either be drawn manually when integrating into other applications
//! or drawn using the default UI implementation [crate::gui::Gui].

mod message_slider;
mod spectrum;
mod waterfall;

pub use message_slider::*;
pub use spectrum::*;
pub use waterfall::*;
