use clap::Parser;
use futuresdr::anyhow::Result;
use futuresdr::blocks::aaronia::HttpSource;
use futuresdr::blocks::Apply;
use futuresdr::blocks::NullSink;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

use zigbee::ClockRecoveryMm;
use zigbee::Decoder;
use zigbee::Mac;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// Url
    #[clap(short, long, default_value = "http://localhost:54664")]
    url: String,
    /// UDP Sink [address:port]
    #[clap(short('U'), long)]
    udp_addr: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("Configuration: {args:?}");

    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    let src = fg.add_block(HttpSource::new(rt.scheduler(), &args.url));

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

    let blob_to_udp = fg.add_block(futuresdr::blocks::BlobToUdp::new("127.0.0.1:55556"));
    fg.connect_message(mac, "rftap", blob_to_udp, "in")?;

    rt.run(fg)?;

    Ok(())
}
