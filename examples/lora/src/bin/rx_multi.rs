use std::collections::HashMap;
use std::time::SystemTime;

use clap::Parser;
use rustfft::num_complex::Complex32;

use futuredsp::firdes::remez;
use futuresdr::anyhow::Result;
use futuresdr::blocks::seify::SourceBuilder;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::PfbArbResampler;
use futuresdr::blocks::PfbChannelizer;
use futuresdr::blocks::StreamDeinterleaver;
use futuresdr::blocks::{BlobToUdp, MessageAnnotator};
use futuresdr::macros::connect;
use futuresdr::runtime::buffer::circular::Circular;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::{Flowgraph, Pmt};
use lora::Deinterleaver;
use lora::FftDemod;
use lora::FrameSync;
use lora::GrayMapping;
use lora::HammingDec;
use lora::HeaderDecoder;
use lora::HeaderMode;
use lora::{Decoder, PacketForwarderClient};

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// RX antenna
    #[clap(long)]
    antenna: Option<String>,
    /// Seify device args
    #[clap(long)]
    args: Option<String>,
    /// RX Gain
    #[clap(long, default_value_t = 50.0)]
    gain: f64,
    /// Socket Address of the Packet Forwarder Server, or None to simply print the frames to stdout
    #[clap(long, default_value = None)]
    forward_addr: Option<String>,
}

const CENTER_FREQ: f64 = 867_900_000.0;
const NUM_CHANNELS: usize = 8;
const NUM_CHANNELS_PADDED: usize = 9;
const CHANNEL_SPACING: usize = 200_000;
const BANDWIDTH: usize = 125_000;
const OVERSAMPLING: usize = 4;
const SOFT_DECODING: bool = false;
const CENTER_FREQS: [u32; NUM_CHANNELS] = [
    867_900_000,
    868_100_000,
    868_300_000,
    868_500_000,
    867_100_000,
    867_300_000,
    867_500_000,
    867_700_000,
];

pub fn map_port(i: usize) -> Option<usize> {
    match i {
        0 => Some(0),
        1 => Some(1),
        2 => Some(2),
        3 => Some(3),
        4 => None,
        5 => Some(4),
        6 => Some(5),
        7 => Some(6),
        8 => Some(7),
        _ => panic!("wrong port number"),
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    // streamer start time is relative to function call -> can not be used for precise rx timestamping -> just use the system time when constructing the flowgraph as a reference
    let stream_start_time = SystemTime::now();

    let src = SourceBuilder::new()
        .sample_rate((NUM_CHANNELS_PADDED * CHANNEL_SPACING) as f64)
        .frequency(CENTER_FREQ)
        .gain(args.gain)
        .antenna(args.antenna)
        .args(args.args)?
        .build()?;

    let deinterleaver = StreamDeinterleaver::<Complex32>::new(NUM_CHANNELS_PADDED);
    connect!(fg, src > deinterleaver);

    let mut tagged_msg_out_ports: Vec<usize> = vec![];

    let transition_bw = (CHANNEL_SPACING - BANDWIDTH) as f64 / CHANNEL_SPACING as f64;
    let channelizer_taps: Vec<f32> = remez::low_pass(
        1.,
        NUM_CHANNELS_PADDED,
        0.5 - transition_bw / 2.,
        0.5 + transition_bw / 2.,
        0.1,
        100.,
        None,
    )
    .into_iter()
    .map(|x| x as f32)
    .collect();
    let channelizer = fg.add_block(PfbChannelizer::new(
        NUM_CHANNELS_PADDED,
        &channelizer_taps,
        1.0,
    ));
    for i in 0..NUM_CHANNELS_PADDED {
        fg.connect_stream(
            deinterleaver,
            format!("out{i}"),
            channelizer,
            format!("in{i}"),
        )?;
    }
    for n_out in 0..NUM_CHANNELS_PADDED {
        let n_chan = map_port(n_out);
        if n_chan.is_none() {
            let null_sink_extra_channel = fg.add_block(NullSink::<Complex32>::new());
            // map highest channel to null-sink (channel numbering starts at center and wraps around)
            fg.connect_stream(
                channelizer,
                format!("out{n_out}"),
                null_sink_extra_channel,
                "in",
            )?;
            println!("connecting channel {n_out} to NullSink");
            continue;
        }
        let n_chan = n_chan.unwrap();

        let resampler_taps: Vec<f32> = remez::low_pass(
            1.,
            5,
            BANDWIDTH as f64 / (2.0 * CHANNEL_SPACING as f64),
            ((BANDWIDTH as f64 / 2.0) + (CHANNEL_SPACING - BANDWIDTH) as f64)
                / (CHANNEL_SPACING as f64),
            0.1,
            100.,
            None,
        )
        .into_iter()
        .map(|x| x as f32)
        .collect();
        let resampler = fg.add_block(PfbArbResampler::new(2.5, &resampler_taps, 5));
        fg.connect_stream(channelizer, format!("out{n_out}"), resampler, "in")?;
        let center_freq = CENTER_FREQS[n_chan] as f32;
        println!(
            "connecting {:.1}MHz chain to channel {}",
            center_freq / 1.0e6,
            n_chan
        );
        for sf in 7..13 {
            println!(
                "connecting {:.1}MHz FrameSync with spreading factor {sf}",
                center_freq / 1.0e6,
            );
            let frame_sync = fg.add_block(FrameSync::new(
                center_freq as u32,
                BANDWIDTH,
                sf,
                false,
                vec![vec![0x12]],
                OVERSAMPLING,
                None,
                None,
                false,
                Some(stream_start_time),
            ));
            fg.connect_stream_with_type(
                resampler,
                "out",
                frame_sync,
                "in",
                Circular::with_size((1 << 12) * 64),
            )?;
            let fft_demod = FftDemod::new(SOFT_DECODING, sf);
            let gray_mapping = GrayMapping::new(SOFT_DECODING);
            let deinterleaver = Deinterleaver::new(SOFT_DECODING);
            let hamming_dec = HammingDec::new(SOFT_DECODING);
            let header_decoder = HeaderDecoder::new(HeaderMode::Explicit, sf >= 12);
            let decoder = fg.add_block(Decoder::new());
            let udp_data = fg.add_block(BlobToUdp::new("127.0.0.1:55555"));
            let udp_rftap = fg.add_block(BlobToUdp::new("127.0.0.1:55556"));

            connect!(fg,
                frame_sync > fft_demod > gray_mapping > deinterleaver > hamming_dec > header_decoder;
                header_decoder.frame_info | frame_sync.frame_info;
                header_decoder | decoder;
                decoder.out | udp_data;
                decoder.rftap | udp_rftap;
            );
            if args.forward_addr.is_some() {
                let tags: HashMap<String, Pmt> = HashMap::from([
                    (String::from("sf"), Pmt::U32(sf as u32)),
                    (String::from("bw"), Pmt::U32((BANDWIDTH / 1000) as u32)),
                    (String::from("freq"), Pmt::F64(center_freq as f64)),
                ]);
                let metadata_tagger = MessageAnnotator::new(tags, None);
                connect!(fg, decoder.out_annotated | metadata_tagger.in);
                tagged_msg_out_ports.push(metadata_tagger);
            }
        }
    }

    if let Some(addr) = args.forward_addr {
        let packet_forwarder =
            fg.add_block(PacketForwarderClient::new("0200.0000.0403.0201", &addr));
        for metadata_tagger in tagged_msg_out_ports {
            connect!(fg, metadata_tagger.out | packet_forwarder.in);
        }
    }

    let _ = rt.run(fg);

    Ok(())
}
