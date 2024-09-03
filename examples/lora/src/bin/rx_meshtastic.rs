use clap::Parser;
use futuredsp::firdes;
use futuresdr::anyhow::Result;
use futuresdr::blocks::seify::SourceBuilder;
use futuresdr::blocks::MessagePipe;
use futuresdr::blocks::XlatingFirBuilder;
use futuresdr::futures::channel::mpsc;
use futuresdr::futures::StreamExt;
use futuresdr::macros::connect;
use futuresdr::runtime::buffer::circular::Circular;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;
use futuresdr::tracing::info;

use lora::meshtastic::MeshtasticChannels;
use lora::meshtastic::MeshtasticConfig;
use lora::utils::Bandwidth;
use lora::Decoder;
use lora::Deinterleaver;
use lora::FftDemod;
use lora::FrameSync;
use lora::GrayMapping;
use lora::HammingDec;
use lora::HeaderDecoder;
use lora::HeaderMode;

const SOFT_DECODING: bool = true;
const IMPLICIT_HEADER: bool = false;
const OVERSAMPLING: usize = 4;

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
    #[clap(long, default_value_t = 50.0)]
    gain: f64,
    /// Meshtastic LoRa Config
    #[clap(long, value_enum, default_value_t = MeshtasticConfig::LongFast)]
    meshtastic_config: MeshtasticConfig,
}

fn main() -> Result<()> {
    futuresdr::runtime::init();
    let args = Args::parse();
    info!("args {:?}", &args);
    let (bandwidth, spreading_factor, _, freq, ldro) = args.meshtastic_config.to_config();

    let src = SourceBuilder::new()
        .sample_rate(1e6)
        .frequency(freq as f64)
        .gain(args.gain)
        .antenna(args.antenna)
        .args(args.args)?
        .build()?;

    let decimation = match bandwidth {
        Bandwidth::BW62 => 4,
        Bandwidth::BW125 => 2,
        Bandwidth::BW250 => 1,
        _ => panic!("wrong bandwidth for Meshtastic"),
    };
    let cutoff = Into::<f64>::into(bandwidth) / 2.0 / 1e6;
    let transition_bw = Into::<f64>::into(bandwidth) / 10.0 / 1e6;
    let taps = firdes::kaiser::lowpass(cutoff, transition_bw, 0.05);
    let decimation = XlatingFirBuilder::with_taps(taps, decimation, 0.0, 1e6);

    let frame_sync = FrameSync::new(
        freq,
        bandwidth.into(),
        spreading_factor.into(),
        IMPLICIT_HEADER,
        vec![vec![16, 88]],
        OVERSAMPLING,
        None,
        Some("header_crc_ok"),
        false,
    );
    let fft_demod = FftDemod::new(SOFT_DECODING, spreading_factor.into());
    let gray_mapping = GrayMapping::new(SOFT_DECODING);
    let deinterleaver = Deinterleaver::new(SOFT_DECODING);
    let hamming_dec = HammingDec::new(SOFT_DECODING);
    let header_decoder = HeaderDecoder::new(HeaderMode::Explicit, ldro);
    let decoder = Decoder::new();

    let (tx_frame, mut rx_frame) = mpsc::channel::<Pmt>(100);
    let message_pipe = MessagePipe::new(tx_frame);

    let mut fg = Flowgraph::new();
    connect!(fg,
        src > decimation [Circular::with_size((1 << 12) * 16 * OVERSAMPLING)] frame_sync;
        frame_sync [Circular::with_size((1 << 12) * 16 * OVERSAMPLING)] fft_demod;
        fft_demod > gray_mapping > deinterleaver > hamming_dec > header_decoder;
        header_decoder.frame_info | frame_sync.frame_info;
        header_decoder | decoder;
        decoder.out | message_pipe;
    );

    let rt = Runtime::new();
    let (_fg, _handle) = rt.start_sync(fg);
    rt.block_on(async move {
        let mut chans = MeshtasticChannels::new();
        chans.add_channel("BBL", "Y203SmFnT1J1SElqRVRqUg==");
        chans.add_channel("FOO", "AQ==");
        chans.add_channel("LALA", "aVJkN3FNQVp6WXFVcGV6Q0NWemxybWlHRFl5RVJkN0U=");
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
