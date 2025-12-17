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
use lora::meshtastic::MeshtasticConfig;
use lora::utils::Bandwidth;

const IMPLICIT_HEADER: bool = false;
const OVERSAMPLING: usize = 4;

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
    #[clap(short, long, value_enum, default_value_t = MeshtasticConfig::LongFastEu)]
    meshtastic_config: MeshtasticConfig,
    /// Meshtastic Channels (Format: <name>:<base64key>,<name>:<base64key>,..)
    #[clap(short, long)]
    channels: Option<String>,
}

fn main() -> Result<()> {
    futuresdr::runtime::init();
    let args = Args::parse();
    info!("args {:?}", &args);
    let (bandwidth, spreading_factor, _, chan, ldro) = args.meshtastic_config.to_config();

    let mut channels = vec![];
    for chan in args.channels.unwrap_or(String::new()).split(",") {
        let vals: Vec<&str> = chan.split(":").collect();
        if vals.len() == 2 {
            channels.push((vals[0].to_string(), vals[1].to_string()));
        }
    }

    println!("channels: {channels:?}");

    let src = Builder::new(args.args)?
        .sample_rate(1e6)
        .frequency(Into::<f64>::into(chan) - 200e3)
        .gain(args.gain)
        .antenna(args.antenna)
        .build_source()?;

    let decimation = match bandwidth {
        Bandwidth::BW62 => 4,
        Bandwidth::BW125 => 2,
        Bandwidth::BW250 => 1,
        _ => panic!("wrong bandwidth for Meshtastic"),
    };
    let cutoff = Into::<f64>::into(bandwidth) / 2.0 / 1e6;
    let transition_bw = Into::<f64>::into(bandwidth) / 10.0 / 1e6;
    let taps = firdes::kaiser::lowpass(cutoff, transition_bw, 0.05);
    let decimation: XlatingFir = XlatingFir::with_taps(taps, decimation, 200e3, 1e6);

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

    let (tx_frame, mut rx_frame) = mpsc::channel::<Pmt>(100);
    let message_pipe = MessagePipe::new(tx_frame);

    let mut fg = Flowgraph::new();
    connect!(fg,
        src.outputs[0] > decimation > frame_sync;
        frame_sync > fft_demod;
        fft_demod > gray_mapping > deinterleaver > hamming_dec > header_decoder;
        header_decoder.frame_info | frame_info.frame_sync;
        header_decoder | decoder;
        decoder | message_pipe;
    );

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
