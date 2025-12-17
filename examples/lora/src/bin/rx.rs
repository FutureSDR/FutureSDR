use anyhow::Result;
use anyhow::anyhow;
use clap::Parser;

use futuresdr::blocks::BlobToUdp;
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
use lora::default_values::ldro;
use lora::utils::Bandwidth;
use lora::utils::Channel;
use lora::utils::ChannelEnumParser;
use lora::utils::SpreadingFactor;

#[derive(Parser, Debug)]
#[clap(version)]
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
    /// RX Channel
    #[clap(long, value_parser=ChannelEnumParser, default_value_t = Channel::EU868_1)]
    channel: Channel,
    /// LoRa Spreading Factor
    #[clap(short, long, value_enum, default_value_t = SpreadingFactor::SF7)]
    spreading_factor: SpreadingFactor,
    /// LoRa Bandwidth
    #[clap(short, long, value_enum, default_value_t = Bandwidth::BW125)]
    bandwidth: Bandwidth,
    /// LoRa Sync Word
    #[clap(long, default_value_t = 0x12)]
    sync_word: u8,
    /// Oversampling Factor
    #[clap(long, default_value_t = 4)]
    oversampling: usize,
}

const IMPLICIT_HEADER: bool = false;

fn main() -> Result<()> {
    futuresdr::runtime::init();
    let args = Args::parse();
    info!("args {:?}", &args);

    let src = Builder::new(args.args)?
        .sample_rate((Into::<usize>::into(args.bandwidth) * args.oversampling) as f64)
        .frequency(args.channel.into())
        .gain(args.gain)
        .antenna(args.antenna)
        .build_source()?;

    let frame_sync: FrameSync = FrameSync::new(
        args.channel,
        args.bandwidth,
        args.spreading_factor,
        IMPLICIT_HEADER,
        vec![vec![args.sync_word.into()]],
        args.oversampling,
        None,
        Some("header_crc_ok"),
        false,
        None,
    );
    let fft_demod: FftDemod = FftDemod::new(args.spreading_factor, ldro(args.spreading_factor));
    let gray_mapping: GrayMapping = GrayMapping::new();
    let deinterleaver: Deinterleaver =
        Deinterleaver::new(ldro(args.spreading_factor), args.spreading_factor);
    let hamming_dec: HammingDecoder = HammingDecoder::new();
    let header_decoder: HeaderDecoder = HeaderDecoder::new(
        if IMPLICIT_HEADER {
            HeaderMode::Implicit {
                payload_len: 15,
                has_crc: false,
                code_rate: 1,
            }
        } else {
            HeaderMode::Explicit
        },
        ldro(args.spreading_factor),
    );
    let decoder: Decoder = Decoder::new();
    let udp_data: BlobToUdp = BlobToUdp::new("127.0.0.1:55555");
    let udp_rftap: BlobToUdp = BlobToUdp::new("127.0.0.1:55556");

    let mut fg = Flowgraph::new();
    connect!(fg,
        src.outputs[0] > frame_sync > fft_demod > gray_mapping > deinterleaver > hamming_dec > header_decoder;
        header_decoder.frame_info | frame_info.frame_sync;
        header_decoder | decoder;
        decoder | udp_data;
        decoder.rftap | udp_rftap;
    );

    if let Err(e) = Runtime::new().run(fg) {
        error!("{}", &e);
        return Err(anyhow!("{}", e));
    }
    Ok(())
}
