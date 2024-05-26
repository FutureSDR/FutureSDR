//! Default GUI implementations for the implemented GUI frontends and
//! corresponding traits

use crate::runtime::{Flowgraph, Runtime};

use crate::anyhow::Result;

#[cfg(feature = "egui")]
mod egui_frontend;
#[cfg(feature = "egui")]
pub use egui_frontend::*;

#[cfg(feature = "textplots")]
mod textplots_frontend;
#[cfg(feature = "textplots")]
pub use textplots_frontend::*;

/// Default `egui` GUI implementation.
#[cfg(feature = "egui")]
pub type Gui = EguiFrontend;

/// Default `textplots` GUI implementation.
#[cfg(feature = "textplots")]
pub type Gui = TextplotsFrontend;

/// GUI frontend agnostic color type
#[derive(Copy, Clone)]
pub struct Color {
    /// red
    pub r: u8,
    /// green
    pub g: u8,
    /// blue
    pub b: u8,
}

#[cfg(feature = "egui")]
impl Into<egui::Color32> for Color {
    fn into(self) -> egui::Color32 {
        egui::Color32::from_rgb(self.r, self.g, self.b)
    }
}

#[cfg(feature = "textplots")]
impl Into<rgb::RGB<u8>> for Color {
    fn into(self) -> rgb::RGB<u8> {
        rgb::RGB::new(self.r, self.g, self.b)
    }
}

/// Type of a GUI widget. Determines the size and placement of the widget
#[derive(PartialEq)]
pub enum GuiWidgetType {
    /// Control widget. Given as little space as possible
    Control,
    /// Plot widget. Given as much space as possible
    Plot,
}

/// Trait implemented by all GUI handles
pub trait GuiWidget: Send {
    /// This widget's type
    fn widget_type(&self) -> GuiWidgetType {
        GuiWidgetType::Plot
    }

    /// Draw the egui UI for the widget. Mutable references to the widgets are
    /// expected to implement [egui::Widget], so this might be `ui.add(self)`.
    #[cfg(feature = "egui")]
    fn egui_ui(&mut self, ui: &mut egui::Ui) -> egui::Response;

    /// Draw the textplots UI for the widget
    #[cfg(feature = "textplots")]
    fn textplots_ui(&mut self, _size: (u16, u16)) {}
}

/// Trait implemented by the default GUI frontend for the currently active
/// GUI feature
pub trait GuiFrontend: Default {
    /// Register a new GUI handle to be displayed
    fn register(&mut self, _widget: Box<dyn GuiWidget + Send>);

    /// Run the GUI with the registered widgets
    fn run_impl(self);

    /// Run the given flowgraph and the corresponding UI. Since egui prefers to
    /// be run on the main thread, this also takes care of spawning a new
    /// thread and runtime for the flowgraph.
    fn run(mut fg: Flowgraph) -> Result<()> {
        let gui_handles = fg.detach_gui_handles();

        if gui_handles.is_empty() {
            let rt = Runtime::new();
            rt.run(fg)?;
        } else {
            std::thread::spawn(move || {
                let rt = Runtime::new();
                rt.run(fg)
            });

            let mut gui = Self::default();
            for handle in gui_handles {
                gui.register(handle);
            }

            gui.run_impl();
        }

        Ok(())
    }
}
