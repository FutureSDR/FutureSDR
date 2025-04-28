use anyhow::Result;
use anyhow::anyhow;
use clap::Parser;
use futuresdr::async_io::Timer;
use futuresdr::blocks::BlobToUdp;
use futuresdr::macros::connect;
use futuresdr::runtime::BlockT;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::buffer::circular::Circular;
use futuresdr::tracing::info;
use std::fmt::Debug;
use std::time::Duration;

use lora::Decoder;
use lora::Deinterleaver;
use lora::FftDemod;
use lora::FrameSync;
use lora::GrayMapping;
use lora::HammingDec;
use lora::HeaderDecoder;
use lora::HeaderMode;
use lora::Transmitter;
use lora::utils::Bandwidth;
use lora::utils::CodeRate;
use lora::utils::SpreadingFactor;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// Send periodic messages for testing
    #[clap(long, default_value_t = 1.0)]
    tx_interval: f32,
    /// Spreading Factor
    #[clap(long, value_enum, default_value_t = SpreadingFactor::SF7)]
    spreading_factor: SpreadingFactor,
    /// Bandwidth
    #[clap(long, value_enum, default_value_t = Bandwidth::BW125)]
    bandwidth: Bandwidth,
    /// Oversampling Factor
    #[clap(long, default_value_t = 1)]
    oversampling: usize,
    /// Sync Word
    #[clap(long, default_value_t = 0x0816)]
    sync_word: usize,
    /// Soft Decoding
    #[clap(long, default_value_t = false)]
    soft_decoding: bool,
    /// LoRa Code Rate
    #[clap(long, value_enum, default_value_t = CodeRate::CR_4_5)]
    code_rate: CodeRate,
}

const HAS_CRC: bool = true;
const IMPLICIT_HEADER: bool = false;
const LOW_DATA_RATE: bool = false;
const PREAMBLE_LEN: usize = 8;
const PAD: usize = 10000;

fn main() -> Result<()> {
    let args = Args::parse();

    let mut fg = Flowgraph::new();

    // ==============================================================
    // TX
    // ==============================================================
    let transmitter = Transmitter::new(
        args.code_rate.into(),
        HAS_CRC,
        args.spreading_factor.into(),
        LOW_DATA_RATE,
        IMPLICIT_HEADER,
        args.oversampling,
        vec![args.sync_word],
        PREAMBLE_LEN,
        PAD,
    );
    let fg_tx_port = transmitter.message_input_name_to_id("msg").ok_or(anyhow!(
        "Message handler `msg` of whitening block not found"
    ))?;

    // ==============================================================
    // RX
    // ==============================================================
    let frame_sync = FrameSync::new(
        868_000_000,
        args.bandwidth.into(),
        args.spreading_factor.into(),
        IMPLICIT_HEADER,
        vec![vec![args.sync_word]],
        args.oversampling,
        None,
        None,
        false,
        None,
    );
    let fft_demod = FftDemod::new(args.soft_decoding, args.spreading_factor.into());
    let gray_mapping = GrayMapping::new(args.soft_decoding);
    let deinterleaver = Deinterleaver::new(args.soft_decoding);
    let hamming_dec = HammingDec::new(args.soft_decoding);
    let header_decoder = HeaderDecoder::new(HeaderMode::Explicit, false);
    let decoder = Decoder::new();
    let udp_data = BlobToUdp::new("127.0.0.1:55555");
    let udp_rftap = BlobToUdp::new("127.0.0.1:55556");
    connect!(fg,
        transmitter [Circular::with_size((1 << usize::from(args.spreading_factor)) * 3 * args.oversampling)] frame_sync > fft_demod > gray_mapping > deinterleaver > hamming_dec > header_decoder;
        header_decoder.frame_info | frame_sync.frame_info;
        header_decoder | decoder;
        decoder.crc_check | frame_sync.payload_crc_result;
        decoder.out | udp_data;
        decoder.rftap | udp_rftap;
    );

    // ==============================================================
    // Send Frames
    // ==============================================================
    let rt = Runtime::new();
    let (_fg, mut handle) = rt.start_sync(fg);
    rt.block_on(async move {
        let mut counter: usize = 0;
        loop {
            let payload = format!("hello world! {counter:02}");
            handle
                .call(transmitter, fg_tx_port, Pmt::String(payload))
                .await
                .unwrap();
            info!("sending frame");
            counter += 1;
            counter %= 100;
            Timer::after(Duration::from_secs_f32(args.tx_interval)).await;
        }
    });

    Ok(())
}
