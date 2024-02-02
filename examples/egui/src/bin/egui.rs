#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use futuresdr::runtime::Pmt;
use futuresdr_remote::Handler;
use futuresdr_remote::Remote;
use std::thread;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;

fn main() -> Result<(), eframe::Error> {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 240.0]),
        ..Default::default()
    };
    eframe::run_native(
        "FutureSDR + egui",
        options,
        Box::new(|_| Box::<MyApp>::default()),
    )
}

enum GuiAction {
    SetFreq(u64),
}

async fn process_gui_actions(
    mut rx: UnboundedReceiver<GuiAction>,
) -> futuresdr::anyhow::Result<()> {
    let remote = Remote::new("http://127.0.0.1:1337");
    let fgs = remote.flowgraphs().await?;
    let sdr = &fgs[0].blocks()[0];

    while let Some(m) = rx.recv().await {
        match m {
            GuiAction::SetFreq(f) => {
                println!("setting frequency to {}", f);
                sdr.callback(Handler::Name("freq".to_string()), Pmt::U64(f))
                    .await?
            }
        };
    }

    Ok(())
}

struct MyApp {
    freq: u64,
    actions: UnboundedSender<GuiAction>,
}

impl Default for MyApp {
    fn default() -> Self {
        let (tx, rx) = unbounded_channel();
        thread::spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(process_gui_actions(rx))
                .unwrap();
        });
        Self {
            freq: 100,
            actions: tx,
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("FutureSDR + egui");
            if ui
                .add(
                    egui::Slider::new(&mut self.freq, 50..=150)
                        .suffix("MHz")
                        .text("frequency"),
                )
                .changed()
            {
                let _ = self.actions.send(GuiAction::SetFreq(self.freq));
            }
            ui.horizontal(|ui| {
                if ui.button("-1").clicked() {
                    self.freq -= 1;
                    let _ = self.actions.send(GuiAction::SetFreq(self.freq));
                }
                if ui.button("+1").clicked() {
                    self.freq += 1;
                    let _ = self.actions.send(GuiAction::SetFreq(self.freq));
                }
            })
        });
    }
}
