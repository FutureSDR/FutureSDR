use anyhow::Result;
use clap::Parser;

use futuredsp::firdes;
use futuresdr::blocks::MessagePipe;
use futuresdr::blocks::XlatingFir;
use futuresdr::blocks::seify::Builder;
use futuresdr::prelude::*;

use lora::Decoder;
use lora::Deinterleaver;
use lora::FftDemod;
use lora::FrameSync;
use lora::GrayMapping;
use lora::HammingDecoder;
use lora::HeaderDecoder;
use lora::HeaderMode;
use lora::meshtastic::MeshtasticChannel;
use lora::meshtastic::MeshtasticChannels;
use lora::utils::Bandwidth;
use lora::utils::Channel;
use lora::utils::SpreadingFactor;

const IMPLICIT_HEADER: bool = false;
const OVERSAMPLING: usize = 4;

#[derive(Debug, Clone, clap::ValueEnum, Copy, Default)]
#[clap(rename_all = "SCREAMING_SNAKE_CASE")]
enum MeshtasticRegion {
    #[default]
    Eu,
    Us,
}

#[derive(Parser, Debug)]
struct Args {
    /// RX Antenna
    #[clap(long)]
    antenna: Option<String>,
    /// Seify Args
    #[clap(short, long)]
    args: Option<String>,
    /// RX Gain
    #[clap(short, long, default_value_t = 50.0)]
    gain: f64,
    /// Meshtastic LoRa Config
    #[clap(short, long, value_enum, default_value_t = MeshtasticRegion::Eu)]
    meshtastic_region: MeshtasticRegion,
    /// Meshtastic Channels (Format: <name>:<base64key>,<name>:<base64key>,..)
    #[clap(short, long)]
    channels: Option<String>,
}

fn main() -> Result<()> {
    futuresdr::runtime::init();
    let args = Args::parse();

    let mut channels = vec![];
    for chan in args.channels.clone().unwrap_or(String::new()).split(",") {
        let vals: Vec<&str> = chan.split(":").collect();
        if vals.len() == 2 {
            channels.push((vals[0].to_string(), vals[1].to_string()));
        }
    }
    info!("args {:?}, channel {:?}", &args, &channels);

    let (sample_rate, center_freq, configs) = match args.meshtastic_region {
        MeshtasticRegion::Eu => (
            1_000_000,
            869_300_000,
            vec![
                (
                    Bandwidth::BW250,
                    Channel::Custom(869_525_000),
                    vec![
                        (SpreadingFactor::SF7, false),
                        (SpreadingFactor::SF8, false),
                        (SpreadingFactor::SF9, false),
                        (SpreadingFactor::SF10, false),
                        (SpreadingFactor::SF11, false),
                    ],
                ),
                (
                    Bandwidth::BW125,
                    Channel::Custom(869_587_500),
                    vec![(SpreadingFactor::SF11, true), (SpreadingFactor::SF12, true)],
                ),
                (
                    Bandwidth::BW62,
                    Channel::Custom(869_492_500),
                    vec![(SpreadingFactor::SF12, true)],
                ),
            ],
        ),
        MeshtasticRegion::Us => (
            20_000_000,
            910_000_000,
            vec![
                (
                    Bandwidth::BW250,
                    Channel::Custom(906_875_000),
                    vec![
                        (SpreadingFactor::SF7, false),
                        (SpreadingFactor::SF8, false),
                        (SpreadingFactor::SF9, false),
                        (SpreadingFactor::SF10, false),
                        (SpreadingFactor::SF11, false),
                    ],
                ),
                (
                    Bandwidth::BW125,
                    Channel::Custom(904_437_500),
                    vec![(SpreadingFactor::SF11, true), (SpreadingFactor::SF12, true)],
                ),
                (
                    Bandwidth::BW62,
                    Channel::Custom(916_218_750),
                    vec![(SpreadingFactor::SF12, true)],
                ),
            ],
        ),
    };

    let mut fg = Flowgraph::new();
    let src = Builder::new(args.args)?
        .sample_rate(sample_rate as f64)
        .frequency(center_freq as f64)
        .gain(args.gain)
        .antenna(args.antenna)
        .build_source()?;

    let (tx_frame, mut rx_frame) = mpsc::channel::<Pmt>(100);
    let message_pipe = MessagePipe::new(tx_frame);
    connect!(fg, src; message_pipe);

    for (bandwidth, chan, chains) in configs.into_iter() {
        let decimation = sample_rate / Into::<usize>::into(bandwidth) / OVERSAMPLING;
        let cutoff = Into::<f64>::into(bandwidth) / 2.0 / sample_rate as f64;
        let transition_bw = cutoff;
        let taps = firdes::kaiser::lowpass(cutoff, transition_bw, 0.05);
        let decimation: XlatingFir = XlatingFir::with_taps(
            taps,
            decimation,
            (Into::<u32>::into(chan) - center_freq) as f32,
            sample_rate as f32,
        );

        let src = src.clone();
        connect!(fg, src.outputs[0] > decimation);

        for (spreading_factor, ldro) in chains.into_iter() {
            let decimation = decimation.clone();
            let message_pipe = message_pipe.clone();
            let frame_sync: FrameSync = FrameSync::new(
                chan,
                bandwidth,
                spreading_factor,
                IMPLICIT_HEADER,
                vec![vec![16, 88]],
                OVERSAMPLING,
                None,
                Some("header_crc_ok"),
                false,
                None,
            );
            let fft_demod: FftDemod = FftDemod::new(spreading_factor, ldro);
            let gray_mapping: GrayMapping = GrayMapping::new();
            let deinterleaver: Deinterleaver = Deinterleaver::new(ldro, spreading_factor);
            let hamming_dec: HammingDecoder = HammingDecoder::new();
            let header_decoder: HeaderDecoder = HeaderDecoder::new(HeaderMode::Explicit, ldro);
            let decoder: Decoder = Decoder::new();
            connect!(fg,
                decimation > frame_sync;
                frame_sync > fft_demod;
                fft_demod > gray_mapping > deinterleaver > hamming_dec > header_decoder;
                header_decoder.frame_info | frame_info.frame_sync;
                header_decoder | decoder;
                decoder | message_pipe;
            );
        }
    }

    let rt = Runtime::new();
    let (_fg, _handle) = rt.start_sync(fg)?;
    rt.block_on(async move {
        let mut chans = MeshtasticChannels::new();
        chans.add_channel(MeshtasticChannel::new("", "AQ=="));
        for c in channels {
            chans.add_channel(MeshtasticChannel::new(&c.0, &c.1));
        }
        while let Some(x) = rx_frame.next().await {
            match x {
                Pmt::Blob(data) => {
                    chans.decode(&data[..data.len() - 2]);
                }
                _ => break,
            }
        }
    });
    Ok(())
}
