use clap::Parser;
use futuresdr::anyhow::Result;
use futuresdr::blocks::seify::SourceBuilder;
use futuresdr::blocks::Apply;
use futuresdr::blocks::BlobToUdp;
use futuresdr::blocks::FileSource;
use futuresdr::blocks::NullSink;
use futuresdr::macros::connect;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

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

    let src = match args.file {
        Some(file) => FileSource::<Complex32>::new(file, false),
        None => SourceBuilder::new()
            .frequency(args.freq)
            .sample_rate(args.sample_rate)
            .gain(args.gain)
            .antenna(args.antenna)
            .args(args.args)?
            .build()?,
    };

    let mut last: Complex32 = Complex32::new(0.0, 0.0);
    let mut iir: f32 = 0.0;
    let alpha = 0.00016;
    let avg = Apply::new(move |i: &Complex32| -> f32 {
        let phase = (last.conj() * i).arg();
        last = *i;
        iir = (1.0 - alpha) * iir + alpha * phase;
        phase - iir
    });

    let omega = 2.0;
    let gain_omega = 0.000225;
    let mu = 0.5;
    let gain_mu = 0.03;
    let omega_relative_limit = 0.0002;
    let mm = ClockRecoveryMm::new(omega, gain_omega, mu, gain_mu, omega_relative_limit);

    let decoder = Decoder::new(6);
    let mac = Mac::new();
    let snk = NullSink::<u8>::new();

    connect!(fg, src > avg > mm > decoder;
                 mac > snk;
                 decoder | mac.rx);

    if let Some(u) = args.udp_addr {
        let blob_to_udp = BlobToUdp::new(u);
        connect!(fg, decoder | blob_to_udp);
    }

    let blob_to_udp = BlobToUdp::new("127.0.0.1:55555");
    connect!(fg, mac.rftap | blob_to_udp);

    Runtime::new().run(fg)?;

    Ok(())
}
