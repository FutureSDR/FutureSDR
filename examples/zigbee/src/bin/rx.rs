use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::seify::Builder;
use futuresdr::blocks::Apply;
use futuresdr::blocks::BlobToUdp;
use futuresdr::blocks::FileSource;
use futuresdr::blocks::NullSink;
use futuresdr::prelude::*;

use zigbee::parse_channel;
use zigbee::ClockRecoveryMm;
use zigbee::Decoder;
use zigbee::Mac;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// File
    #[clap(short, long)]
    file: Option<String>,
    /// Antenna
    #[clap(long)]
    antenna: Option<String>,
    /// Seify Args
    #[clap(short, long)]
    args: Option<String>,
    /// Gain
    #[clap(short, long, default_value_t = 30.0)]
    gain: f64,
    /// Sample Rate
    #[clap(short, long, default_value_t = 4e6)]
    sample_rate: f64,
    /// Zigbee Channel Number (11..26)
    #[clap(id = "channel", short, long, value_parser = parse_channel, default_value = "26")]
    freq: f64,
    /// UDP Sink [address:port]
    #[clap(short, long)]
    udp_addr: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("Configuration: {args:?}");

    let mut fg = Flowgraph::new();

    let (src, output): (BlockId, _) = match args.file {
        Some(file) => (
            fg.add_block(FileSource::<Complex32>::new(file, false))
                .into(),
            "output",
        ),
        None => (
            fg.add_block(
                Builder::new(args.args)?
                    .frequency(args.freq)
                    .sample_rate(args.sample_rate)
                    .gain(args.gain)
                    .antenna(args.antenna)
                    .build_source()?,
            )
            .into(),
            "outputs[0]",
        ),
    };

    let mut last: Complex32 = Complex32::new(0.0, 0.0);
    let mut iir: f32 = 0.0;
    let alpha = 0.00016;
    let avg = fg.add_block(Apply::<_, _, _>::new(move |i: &Complex32| -> f32 {
        let phase = (last.conj() * i).arg();
        last = *i;
        iir = (1.0 - alpha) * iir + alpha * phase;
        phase - iir
    }));

    fg.connect_dyn(src, output, &avg, "input")?;

    let omega = 2.0;
    let gain_omega = 0.000225;
    let mu = 0.5;
    let gain_mu = 0.03;
    let omega_relative_limit = 0.0002;
    let mm: ClockRecoveryMm =
        ClockRecoveryMm::new(omega, gain_omega, mu, gain_mu, omega_relative_limit);

    let decoder = Decoder::new(6);
    let mac: Mac = Mac::new();
    let snk = NullSink::<u8>::new();

    connect!(fg, avg > mm > decoder;
                 mac > snk;
                 decoder | rx.mac);

    if let Some(u) = args.udp_addr {
        let blob_to_udp = BlobToUdp::new(u);
        connect!(fg, decoder | blob_to_udp);
    }

    let blob_to_udp = BlobToUdp::new("127.0.0.1:55555");
    connect!(fg, mac.rftap | blob_to_udp);

    Runtime::new().run(fg)?;

    Ok(())
}
