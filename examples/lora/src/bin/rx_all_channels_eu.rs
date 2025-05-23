use anyhow::Result;
use clap::Parser;
use futuredsp::firdes::remez;
use futuresdr::blocks::seify::Builder;
use futuresdr::blocks::BlobToUdp;
use futuresdr::blocks::MessageAnnotator;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::PfbArbResampler;
use futuresdr::blocks::PfbChannelizer;
use futuresdr::blocks::StreamDeinterleaver;
use futuresdr::prelude::*;
use std::collections::HashMap;
use std::time::SystemTime;

use lora::Decoder;
use lora::Deinterleaver;
use lora::FftDemod;
use lora::FrameSync;
use lora::GrayMapping;
use lora::HammingDec;
use lora::HeaderDecoder;
use lora::HeaderMode;
use lora::PacketForwarderClient;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// RX antenna
    #[clap(long)]
    antenna: Option<String>,
    /// Seify device args
    #[clap(short, long)]
    args: Option<String>,
    /// RX Gain
    #[clap(short, long, default_value_t = 50.0)]
    gain: f64,
    /// Socket Address of the Packet Forwarder Server, or None to simply print the frames to stdout
    #[clap(short, long)]
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

    let packet_forwarder = args
        .forward_addr
        .map(|addr| fg.add_block(PacketForwarderClient::new("0200.0000.0403.0201", &addr)));

    let src = Builder::new(args.args)?
        .sample_rate((NUM_CHANNELS_PADDED * CHANNEL_SPACING) as f64)
        .frequency(CENTER_FREQ)
        .gain(args.gain)
        .antenna(args.antenna)
        .build_source()?;

    let deinterleaver = StreamDeinterleaver::<Complex32>::new(NUM_CHANNELS_PADDED);
    connect!(fg, src.outputs[0] > deinterleaver);

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
    let channelizer: PfbChannelizer =
        PfbChannelizer::new(NUM_CHANNELS_PADDED, &channelizer_taps, 1.0);
    let channelizer = fg.add_block(channelizer);
    for i in 0..NUM_CHANNELS_PADDED {
        fg.connect_dyn(
            &deinterleaver,
            format!("out{i}"),
            &channelizer,
            format!("in{i}"),
        )?;
    }
    for n_out in 0..NUM_CHANNELS_PADDED {
        let n_chan = map_port(n_out);
        if n_chan.is_none() {
            let null_sink_extra_channel = fg.add_block(NullSink::<Complex32>::new());
            // map highest channel to null-sink (channel numbering starts at center and wraps around)
            fg.connect_dyn(
                &channelizer,
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
        fg.connect_dyn(&channelizer, format!("out{n_out}"), &resampler, "in")?;
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
            let frame_sync: FrameSync = FrameSync::new(
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
            );
            let frame_sync = fg.add_block(frame_sync);
            fg.connect_dyn(&resampler, "out", &frame_sync, "in")?;
            let fft_demod: FftDemod = FftDemod::new(SOFT_DECODING, sf);
            let gray_mapping: GrayMapping = GrayMapping::new(SOFT_DECODING);
            let deinterleaver: Deinterleaver = Deinterleaver::new(SOFT_DECODING);
            let hamming_dec: HammingDec = HammingDec::new(SOFT_DECODING);
            let header_decoder = HeaderDecoder::new(HeaderMode::Explicit, sf >= 12);
            let decoder = Decoder::new();
            let udp_data = BlobToUdp::new("127.0.0.1:55555");
            let udp_rftap = BlobToUdp::new("127.0.0.1:55556");

            connect!(fg,
                frame_sync > fft_demod > gray_mapping > deinterleaver > hamming_dec > header_decoder;
                header_decoder.frame_info | frame_info.frame_sync;
                header_decoder | decoder;
                decoder.out | udp_data;
                decoder.rftap | udp_rftap;
            );
            if let Some(ref pf) = packet_forwarder {
                let packet_forwarder = pf.clone();
                let tags: HashMap<String, Pmt> = HashMap::from([
                    (String::from("sf"), Pmt::U32(sf as u32)),
                    (String::from("bw"), Pmt::U32((BANDWIDTH / 1000) as u32)),
                    (String::from("freq"), Pmt::F64(center_freq as f64)),
                ]);
                let metadata_tagger = MessageAnnotator::new(tags, None);
                connect!(fg, decoder.out_annotated | metadata_tagger);
                connect!(fg, metadata_tagger | packet_forwarder);
            }
        }
    }

    rt.run(fg)?;

    Ok(())
}
