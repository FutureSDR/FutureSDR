use eframe;

use crate::gui::{GuiFrontend, GuiWidget, GuiWidgetType};

/// The default GUI implementation when using the `egui` feature
#[derive(Default)]
pub struct EguiFrontend {
    controls: Vec<Box<dyn GuiWidget>>,
    plots: Vec<Box<dyn GuiWidget>>,
}

impl eframe::App for EguiFrontend {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // TODO: layout
            ui.columns(self.controls.len(), |columns| {
                for (i, control) in self.controls.iter_mut().enumerate() {
                    control.egui_ui(&mut columns[i]);
                }
            });

            egui::Grid::new(ui.next_auto_id())
                .min_col_width(ui.available_width())
                .min_row_height(ui.available_height() / (self.plots.len() as f32))
                .show(ui, |ui| {
                    for plot in self.plots.iter_mut() {
                        ui.allocate_ui(ui.available_size(), |ui| {
                            plot.egui_ui(ui);
                        });
                        ui.end_row();
                    }
                });
        });
    }
}

impl GuiFrontend for EguiFrontend {
    fn register(&mut self, widget: Box<dyn GuiWidget + Send>) {
        if widget.widget_type() == GuiWidgetType::Control {
            self.controls.push(widget);
        } else {
            self.plots.push(widget);
        }
    }

    fn run_impl(self) {
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([1000.0, 1200.0]),
            multisampling: 4,
            renderer: eframe::Renderer::Glow,
            ..Default::default()
        };

        // TODO: if we actually do something with the cc, we probably need
        // to create another app struct and implement eframe::App for that
        // instead.
        eframe::run_native("FutureSDR", options, Box::new(|_cc| Box::new(self))).unwrap();
    }
}
