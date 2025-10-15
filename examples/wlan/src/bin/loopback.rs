use anyhow::Result;
use futuresdr::async_io::Timer;
use futuresdr::blocks::Apply;
use futuresdr::blocks::BlobToUdp;
use futuresdr::blocks::Combine;
use futuresdr::blocks::Delay;
use futuresdr::blocks::Fft;
use futuresdr::blocks::FftDirection;
use futuresdr::blocks::MessagePipe;
use futuresdr::blocks::WebsocketPmtSink;
use futuresdr::futures::StreamExt;
use futuresdr::prelude::*;
use rand_distr::Distribution;
use rand_distr::Normal;
use std::time::Duration;

use wlan::Decoder;
use wlan::Encoder;
use wlan::FrameEqualizer;
use wlan::Mac;
use wlan::Mapper;
use wlan::Mcs;
use wlan::MovingAverage;
use wlan::Prefix;
use wlan::SyncLong;
use wlan::SyncShort;

const PAD_FRONT: usize = 10000;
const PAD_TAIL: usize = 10000;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();
    let mac = Mac::new([0x42; 6], [0x23; 6], [0xff; 6]);
    let encoder: Encoder = Encoder::new(Mcs::Qpsk_1_2);
    connect!(fg, mac.tx | tx.encoder);
    let mapper: Mapper = Mapper::new();
    connect!(fg, encoder > mapper);
    let fft: Fft = Fft::with_options(
        64,
        FftDirection::Inverse,
        true,
        Some((1.0f32 / 52.0).sqrt()),
    );
    connect!(fg, mapper > fft);
    let prefix: Prefix = Prefix::new(PAD_FRONT, PAD_TAIL);
    connect!(fg, fft > prefix);

    // add noise
    let normal = Normal::new(0.0f32, 0.01).unwrap();
    let noise = Apply::<_, _, _>::new(move |i: &Complex32| -> Complex32 {
        let re = normal.sample(&mut rand::rng());
        let imag = normal.sample(&mut rand::rng());
        i + Complex32::new(re, imag)
    });
    connect!(fg, prefix > noise);

    let src = noise;

    // ========================================
    // Receiver
    // ========================================
    let delay = Delay::<Complex32>::new(16);
    connect!(fg, src > delay);

    let complex_to_mag_2 = Apply::<_, _, _>::new(|i: &Complex32| i.norm_sqr());
    let float_avg = MovingAverage::<f32>::new(64);
    connect!(fg, src > complex_to_mag_2 > float_avg);

    let mult_conj = Combine::<_, _, _, _>::new(|a: &Complex32, b: &Complex32| a * b.conj());
    let complex_avg = MovingAverage::<Complex32>::new(48);
    connect!(fg, src > in0.mult_conj > complex_avg);
    connect!(fg, delay > in1.mult_conj);

    let divide_mag = Combine::<_, _, _, _>::new(|a: &Complex32, b: &f32| a.norm() / b);
    connect!(fg, complex_avg > in0.divide_mag);
    connect!(fg, float_avg > in1.divide_mag);

    let sync_short: SyncShort = SyncShort::new();
    connect!(fg, delay > in_sig.sync_short);
    connect!(fg, complex_avg > in_abs.sync_short);
    connect!(fg, divide_mag > in_cor.sync_short);

    let sync_long: SyncLong = SyncLong::new();
    connect!(fg, sync_short > sync_long);

    let fft: Fft = Fft::new(64);
    connect!(fg, sync_long > fft);

    let frame_equalizer: FrameEqualizer = FrameEqualizer::new();
    connect!(fg, fft > frame_equalizer);

    let symbol_sink = WebsocketPmtSink::new(9002);
    let decoder = Decoder::new();
    connect!(fg, frame_equalizer > decoder);
    connect!(fg, frame_equalizer.symbols | symbol_sink);

    let (tx_frame, mut rx_frame) = mpsc::channel::<Pmt>(100);
    let message_pipe = MessagePipe::new(tx_frame);
    connect!(fg, decoder.rx_frames | message_pipe);
    let blob_to_udp = BlobToUdp::new("127.0.0.1:55555");
    connect!(fg, decoder.rx_frames | blob_to_udp);
    let blob_to_udp = BlobToUdp::new("127.0.0.1:55556");
    connect!(fg, decoder.rftap | blob_to_udp);
    let mac = mac.get()?.id;

    let rt = Runtime::new();
    let (_fg, mut handle) = rt.start_sync(fg)?;

    let mut seq = 0u64;
    rt.spawn_background(async move {
        loop {
            Timer::after(Duration::from_secs_f32(0.8)).await;
            handle
                .call(
                    mac,
                    "tx",
                    Pmt::Any(Box::new((
                        format!("FutureSDR {seq}").as_bytes().to_vec(),
                        Mcs::Qam16_1_2,
                    ))),
                )
                .await
                .unwrap();
            seq += 1;
        }
    });

    rt.block_on(async move {
        while let Some(x) = rx_frame.next().await {
            match x {
                Pmt::Blob(data) => {
                    println!("received frame ({:?} bytes)", data.len());
                }
                _ => break,
            }
        }
    });

    Ok(())
}
