use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::gui::{GuiFrontend, GuiWidget, GuiWidgetType};

/// The default GUI implementation when using the `textplots` feature
#[derive(Default)]
pub struct TextplotsFrontend {
    plots: Vec<Box<dyn GuiWidget>>,
}

impl GuiFrontend for TextplotsFrontend {
    fn register(&mut self, widget: Box<dyn GuiWidget + Send>) {
        if widget.widget_type() != GuiWidgetType::Control {
            self.plots.push(widget);
        }
    }

    fn run_impl(mut self) {
        let term = console::Term::stdout();
        term.hide_cursor().unwrap();
        term.clear_screen().unwrap();

        let should_run = Arc::new(AtomicBool::new(true));
        let should_run2 = should_run.clone();
        ctrlc::set_handler(move || {
            should_run2.as_ref().store(false, Ordering::Relaxed);
        })
        .unwrap();

        while should_run.as_ref().load(Ordering::Acquire) {
            term.clear_screen().unwrap();

            let (rows, columns) = term.size();
            let plot_height = (rows as usize) / self.plots.len();

            for (i, plot) in self.plots.iter_mut().enumerate() {
                term.move_cursor_to(0, i * plot_height).unwrap();
                plot.textplots_ui((columns, plot_height as u16));
            }

            std::thread::sleep(Duration::from_millis(16));
        }

        term.show_cursor().unwrap();
    }
}
