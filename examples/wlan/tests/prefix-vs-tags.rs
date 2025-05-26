use futuresdr::blocks::Fft;
use futuresdr::blocks::FftDirection;
use futuresdr::blocks::MessageBurst;
use futuresdr::blocks::NullSink;
use futuresdr::prelude::*;

use wlan::Encoder;
use wlan::Mac;
use wlan::Mapper;
use wlan::Mcs;
use wlan::Prefix;

#[test]
fn tags_vs_prefix() -> Result<()> {
    let mut fg = Flowgraph::new();
    let burst = MessageBurst::new(Pmt::Blob("lol".as_bytes().to_vec()), 1000);
    let mac = Mac::new([0x42; 6], [0x23; 6], [0xff; 6]);
    connect!(fg, burst | tx.mac);
    let encoder: Encoder = Encoder::new(Mcs::Qpsk_1_2);
    connect!(fg, mac.tx | tx.encoder);
    let mapper: Mapper = Mapper::new();
    connect!(fg, encoder > mapper);
    let fft: Fft = Fft::with_options(
        64,
        FftDirection::Inverse,
        true,
        Some((1.0f32 / 52.0).sqrt() * 0.6),
    );
    connect!(fg, mapper > fft);
    let prefix: Prefix = Prefix::new(0, 0);
    connect!(fg, fft > prefix);
    let snk = NullSink::<Complex32>::new();
    connect!(fg, prefix > snk);

    let _ = Runtime::new().run(fg);

    Ok(())
}
