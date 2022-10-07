use clap::Parser;
use futuresdr::anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::SoapySourceBuilder;
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
    /// Antenna
    #[clap(short, long)]
    antenna: Option<String>,
    /// Soapy Filter
    #[clap(short, long)]
    filter: Option<String>,
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
    println!("Configuration: {:?}", args);

    let mut fg = Flowgraph::new();

    let mut soapy_src = SoapySourceBuilder::new()
        .freq(args.freq)
        .sample_rate(args.sample_rate)
        .gain(args.gain);
    if let Some(a) = args.antenna {
        soapy_src = soapy_src.antenna(a);
    }
    if let Some(f) = args.filter {
        soapy_src = soapy_src.filter(f);
    }

    let src = fg.add_block(soapy_src.build());

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

    if let Some(u) = args.udp_addr {
        let blob_to_udp = fg.add_block(futuresdr::blocks::BlobToUdp::new(u));
        fg.connect_message(decoder, "out", blob_to_udp, "in")?;
    }

    Runtime::new().run(fg)?;

    Ok(())
}
