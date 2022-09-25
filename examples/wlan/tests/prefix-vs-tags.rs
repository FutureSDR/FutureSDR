use futuresdr::anyhow::Result;
use futuresdr::blocks::Fft;
use futuresdr::blocks::FftDirection;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::MessageBurst;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::buffer::circular::Circular;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;

use wlan::fft_tag_propagation;
use wlan::Encoder;
use wlan::Mac;
use wlan::Mapper;
use wlan::Mcs;
use wlan::Prefix;

use wlan::MAX_SYM;
const PAD_FRONT: usize = 5000;
const PAD_TAIL: usize = 5000;

#[test]
fn tags_vs_prefix() -> Result<()> {
    let mut size = 4096;
    let prefix_in_size = loop {
        if size / 8 >= MAX_SYM * 64 {
            break size;
        }
        size += 4096
    };
    let mut size = 4096;
    let prefix_out_size = loop {
        if size / 8 >= PAD_FRONT + std::cmp::max(PAD_TAIL, 1) + 320 + MAX_SYM * 80 {
            break size;
        }
        size += 4096
    };

    let mut fg = Flowgraph::new();
    let burst = fg.add_block(MessageBurst::new(Pmt::Blob("lol".as_bytes().to_vec()), 1000));
    let mac = fg.add_block(Mac::new([0x42; 6], [0x23; 6], [0xff; 6]));
    fg.connect_message(burst, "out", mac, "tx")?;
    let encoder = fg.add_block(Encoder::new(Mcs::Qpsk_1_2));
    fg.connect_message(mac, "tx", encoder, "tx")?;
    let mapper = fg.add_block(Mapper::new());
    fg.connect_stream(encoder, "out", mapper, "in")?;
    let mut fft = Fft::with_options(
        64,
        FftDirection::Inverse,
        true,
        Some((1.0f32 / 52.0).sqrt() * 0.6),
    );
    fft.set_tag_propagation(Box::new(fft_tag_propagation));
    let fft = fg.add_block(fft);
    fg.connect_stream(mapper, "out", fft, "in")?;
    let prefix = fg.add_block(Prefix::new(PAD_FRONT, PAD_TAIL));
    fg.connect_stream_with_type(
        fft,
        "out",
        prefix,
        "in",
        Circular::with_size(prefix_in_size),
    )?;
    let snk = fg.add_block(NullSink::<Complex32>::new());
    fg.connect_stream_with_type(
        prefix,
        "out",
        snk,
        "in",
        Circular::with_size(prefix_out_size),
    )?;

    let _ = Runtime::new().run(fg);

    Ok(())
}
