use anyhow::anyhow;
use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::seify::SourceBuilder;
use futuresdr::blocks::BlobToUdp;
use futuresdr::macros::connect;
use futuresdr::runtime::buffer::circular::Circular;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use futuresdr::tracing::error;
use futuresdr::tracing::info;

use lora::utils::Bandwidth;
use lora::utils::SpreadingFactor;
use lora::Decoder;
use lora::Deinterleaver;
use lora::FftDemod;
use lora::FrameSync;
use lora::GrayMapping;
use lora::HammingDec;
use lora::HeaderDecoder;
use lora::HeaderMode;

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
    /// Channel Frequency
    #[clap(short, long)]
    freq: f64,
    /// LoRa Spreading Factor
    #[clap(short, long, value_enum, default_value_t = SpreadingFactor::SF7)]
    spreading_factor: SpreadingFactor,
    /// LoRa Bandwidth
    #[clap(short, long, value_enum, default_value_t = Bandwidth::BW125)]
    bandwidth: Bandwidth,
}

const OVERSAMPLING: usize = 4;
const SOFT_DECODING: bool = false;
const IMPLICIT_HEADER: bool = false;

fn main() -> Result<()> {
    futuresdr::runtime::init();
    let args = Args::parse();
    info!("args {:?}", &args);

    let src = SourceBuilder::new()
        .sample_rate(Into::<f64>::into(args.bandwidth) * OVERSAMPLING as f64)
        .frequency(args.freq)
        .gain(args.gain)
        .antenna(args.antenna)
        .args(args.args)?
        .build()?;

    let frame_sync = FrameSync::new(
        args.freq as u32,
        args.bandwidth.into(),
        args.spreading_factor.into(),
        IMPLICIT_HEADER,
        vec![],
        OVERSAMPLING,
        None,
        Some("header_crc_ok"),
        false,
        None,
    );
    let fft_demod = FftDemod::new(SOFT_DECODING, args.spreading_factor.into());
    let gray_mapping = GrayMapping::new(SOFT_DECODING);
    let deinterleaver = Deinterleaver::new(SOFT_DECODING);
    let hamming_dec = HammingDec::new(SOFT_DECODING);
    let header_decoder = HeaderDecoder::new(HeaderMode::Explicit, false);
    let decoder = Decoder::new();
    let udp_data = BlobToUdp::new("127.0.0.1:55555");
    let udp_rftap = BlobToUdp::new("127.0.0.1:55556");

    let mut fg = Flowgraph::new();
    connect!(fg,
        src [Circular::with_size(2 * 4 * 8192 * 4)] frame_sync > fft_demod > gray_mapping > deinterleaver > hamming_dec > header_decoder;
        header_decoder.frame_info | frame_sync.frame_info;
        header_decoder | decoder;
        decoder.out | udp_data;
        decoder.rftap | udp_rftap;
    );

    if let Err(e) = Runtime::new().run(fg) {
        error!("{}", &e);
        return Err(anyhow!("{}", e));
    }
    Ok(())
}
