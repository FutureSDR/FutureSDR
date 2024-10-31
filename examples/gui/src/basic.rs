#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use futuresdr::anyhow::Result;
use futuresdr::blocks::gui::*;
use futuresdr::blocks::seify::SourceBuilder;
use futuresdr::blocks::{MessageCopy, Split};
use futuresdr::gui::{Gui, GuiFrontend};
use futuresdr::macros::connect;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::seify::Direction;

// Initial values. In theory, sample rate could also be configured via GUI, but
// most drivers really don't like quick updates at runtime.
const SAMPLE_RATE: f64 = 20e6;
const FREQUENCY: f64 = 868e6;
const GAIN: f64 = 50.0;

fn main() -> Result<()> {
    env_logger::init();

    let device = futuresdr::seify::Device::new().expect("Failed to find a Seify device");
    let freq_range = device.frequency_range(Direction::Rx, 0).unwrap();
    let gain_range = device.gain_range(Direction::Rx, 0).unwrap();

    let mut fg = Flowgraph::new();

    let src = SourceBuilder::new()
        .device(device)
        .frequency(FREQUENCY)
        .sample_rate(SAMPLE_RATE)
        .gain(GAIN)
        .build()
        .unwrap();

    let split = Split::new(|x: &Complex32| (*x, *x));

    let spectrum = SpectrumPlotBuilder::new(SAMPLE_RATE)
        .center_frequency(FREQUENCY)
        .fft_size(2048)
        .build();

    let waterfall = WaterfallBuilder::new(SAMPLE_RATE)
        .center_frequency(FREQUENCY)
        .build();

    let freq_min = freq_range.at_least(0.0).unwrap() / 1e6;
    let freq_max = freq_range.at_max(1e12).unwrap() / 1e6;
    let freq_slider = MessageSliderBuilder::<f64>::new(freq_min..=freq_max)
        .step_size(0.1)
        .initial_value(FREQUENCY / 1e6)
        .label("Frequency")
        .suffix("MHz")
        .multiplier(1e6)
        .build();

    let gain_min = gain_range.at_least(-1000.0).unwrap();
    let gain_max = gain_range.at_max(1000.0).unwrap();
    let gain_slider = MessageSliderBuilder::<f64>::new(gain_min..=gain_max)
        .step_size(0.1)
        .initial_value(GAIN)
        .label("Gain")
        .suffix("dB")
        .build();

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

    Gui::run(fg) // instead of rt.run(fg)
}
