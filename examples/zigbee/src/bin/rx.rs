use clap::{Arg, Command};
use futuresdr::anyhow::{Context, Result};
use futuresdr::blocks::Apply;
use futuresdr::blocks::SoapySourceBuilder;
use futuresdr::blocks::NullSink;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

use zigbee::ClockRecoveryMm;
use zigbee::Decoder;
use zigbee::Mac;
use zigbee::channel_to_freq;

fn main() -> Result<()> {
    let matches = Command::new("ZigBee Receiver")
        .arg(
            Arg::new("channel")
                .short('c')
                .long("channel")
                .takes_value(true)
                .value_name("CHANNEL")
                .default_value("26")
                .help("Channel (11..=26)."),
        )
        .get_matches();

    let channel: u32 = matches.value_of_t("channel").context("invalid channel")?;
    let freq = channel_to_freq(channel)?;

    let mut fg = Flowgraph::new();

    let src = fg.add_block(
        SoapySourceBuilder::new()
            .freq(freq)
            .sample_rate(4e6)
            .gain(60.0)
            .build(),
    );

    let mut last: Complex32 = Complex32::new(0.0, 0.0);
    let mut iir: f32 = 0.0;
    let alpha = 0.00016;
    let avg = fg.add_block(Apply::new(move |i: &Complex32| -> f32 {
        let phase = (last.conj() * i).arg();
        last = *i;
        iir = (1.0 - alpha) * iir + alpha * phase;
        phase - iir
    }));

    let omega = 2.0;
    let gain_omega = 0.000225;
    let mu = 0.5;
    let gain_mu = 0.03;
    let omega_relative_limit = 0.0002;
    let mm = fg.add_block(ClockRecoveryMm::new(
        omega,
        gain_omega,
        mu,
        gain_mu,
        omega_relative_limit,
    ));

    let decoder = fg.add_block(Decoder::new(6));
    let mac = fg.add_block(Mac::new());
    let snk = fg.add_block(NullSink::<u8>::new());

    fg.connect_stream(src, "out", avg, "in")?;
    fg.connect_stream(avg, "out", mm, "in")?;
    fg.connect_stream(mm, "out", decoder, "in")?;
    fg.connect_stream(mac, "out", snk, "in")?;
    fg.connect_message(decoder, "out", mac, "rx")?;

    Runtime::new().run(fg)?;

    Ok(())
}
