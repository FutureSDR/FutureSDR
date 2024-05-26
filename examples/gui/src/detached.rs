#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::thread;

use futuresdr::anyhow::Result;
use futuresdr::blocks::seify::SourceBuilder;
use futuresdr::blocks::{MessageCopy, Split};
use futuresdr::blocks::gui::*;
use futuresdr::macros::connect;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use futuresdr::seify::Direction;

use eframe::egui;

// Initial values. In theory, sample rate could also be configured via GUI, but
// most drivers really don't like quick updates at runtime.
const SAMPLE_RATE: f64 = 20e6;
const FREQUENCY: f64 = 868e6;
const GAIN: f64 = 50.0;

struct MyApp {
    device_id: String,
    device_args: futuresdr::seify::Args,
    frequency_slider_handle: MessageSliderHandle<f64>,
    gain_slider_handle: MessageSliderHandle<f64>,
    spectrum_plot_handle: SpectrumPlotHandle,
    waterfall_handle: WaterfallHandle,
}

impl MyApp {
    fn build_flowgraph() -> Result<(Flowgraph, Self)> {
        let device = futuresdr::seify::Device::new().expect("Failed to find a Seify device");
        let device_id = device.id()?;
        let device_args = device.info()?;
        let freq_range = device.frequency_range(Direction::Rx, 0)?;
        let gain_range = device.gain_range(Direction::Rx, 0)?;

        let mut fg = Flowgraph::new();

        let src = SourceBuilder::new()
            .device(device)
            .frequency(FREQUENCY)
            .sample_rate(SAMPLE_RATE)
            .gain(GAIN)
            .build()
            .unwrap();

        let split = Split::new(|x: &Complex32| (*x, *x));

        let (spectrum, spectrum_handle) = SpectrumPlotBuilder::new(SAMPLE_RATE)
            .center_frequency(FREQUENCY)
            .fft_size(2048)
            .build_detached();

        let (waterfall, waterfall_handle) = WaterfallBuilder::new(SAMPLE_RATE)
            .center_frequency(FREQUENCY)
            .build_detached();

        let freq_min = freq_range.at_least(0.0).unwrap() / 1e6;
        let freq_max = freq_range.at_max(1e12).unwrap() / 1e6;
        let (freq_slider, freq_handle) = MessageSliderBuilder::<f64>::new(freq_min..=freq_max)
            .step_size(0.1)
            .initial_value(FREQUENCY / 1e6)
            .label("Frequency")
            .suffix("MHz")
            .multiplier(1e6)
            .build_detached();

        let gain_min = gain_range.at_least(-1000.0).unwrap();
        let gain_max = gain_range.at_max(1000.0).unwrap();
        let (gain_slider, gain_handle) = MessageSliderBuilder::<f64>::new(gain_min..=gain_max)
            .step_size(0.1)
            .initial_value(GAIN)
            .label("Gain")
            .suffix("dB")
            .build_detached();

        // A whole bunch of blocks need access to the center frequency so we push all changes
        // in here for convenience, so we don't have to do point-to-point connections.
        let center_freq_hub = MessageCopy::new();

        connect!(fg,
                 src > split;
                 split.out0 > spectrum;
                 split.out1 > waterfall;
                 freq_slider | center_freq_hub | freq_slider;
                 waterfall.drag_freq | center_freq_hub | waterfall.center_freq;
                 spectrum.drag_freq | center_freq_hub | spectrum.center_freq;
                 center_freq_hub | src.freq;
                 gain_slider | src.gain);

        let app = Self {
            device_id,
            device_args,
            frequency_slider_handle: freq_handle,
            gain_slider_handle: gain_handle,
            spectrum_plot_handle: spectrum_handle,
            waterfall_handle,
        };

        Ok((fg, app))
    }

    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let (fg, app) = Self::build_flowgraph().unwrap();

        thread::spawn(move || -> Result<()> {
            let rt = Runtime::new();
            let _ = rt.run(fg);
            Ok(())
        });

        app
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("sdr_settings").show(ctx, |ui| {
            ui.heading(format!("Device #{}", self.device_id));
            for (key, value) in self.device_args.iter() {
                ui.monospace(format!("{}: {}", key, value));
            }

            ui.separator();

            ui.add(&mut self.frequency_slider_handle);
            ui.add(&mut self.gain_slider_handle);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let size = egui::Vec2::new(ui.available_width(), ui.available_height() * 0.3);
            ui.allocate_ui(size, |ui| {
                ui.add(&mut self.spectrum_plot_handle);
            });

            ui.add(&mut self.waterfall_handle);
        });
    }
}

fn main() -> Result<(), eframe::Error> {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1000.0, 1200.0]),
        multisampling: 4,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "FutureSDR GUI Demo",
        options,
        Box::new(|cc| Box::new(MyApp::new(cc))),
    )
}
