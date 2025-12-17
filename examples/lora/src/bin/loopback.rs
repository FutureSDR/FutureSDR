use std::fmt::Debug;
use std::time::Duration;

use clap::Parser;

use futuresdr::async_io::Timer;
use futuresdr::blocks::BlobToUdp;
use futuresdr::prelude::*;

use lora::Decoder;
use lora::Deinterleaver;
use lora::FftDemod;
use lora::FrameSync;
use lora::GrayMapping;
use lora::HammingDecoder;
use lora::HeaderDecoder;
use lora::HeaderMode;
use lora::Transmitter;
use lora::default_values::ldro;
use lora::utils::Bandwidth;
use lora::utils::Channel;
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
    let transmitter: Transmitter = Transmitter::new(
        args.code_rate,
        HAS_CRC,
        args.spreading_factor,
        LOW_DATA_RATE,
        IMPLICIT_HEADER,
        args.oversampling,
        vec![args.sync_word],
        PREAMBLE_LEN,
        PAD,
    );

    // ==============================================================
    // RX
    // ==============================================================
    let frame_sync: FrameSync = FrameSync::new(
        Channel::EU868_1,
        args.bandwidth,
        args.spreading_factor,
        IMPLICIT_HEADER,
        vec![vec![args.sync_word]],
        args.oversampling,
        None,
        None,
        false,
        None,
    );
    let fft_demod: FftDemod = FftDemod::new(args.spreading_factor, ldro(args.spreading_factor));
    let gray_mapping: GrayMapping = GrayMapping::new();
    let deinterleaver: Deinterleaver =
        Deinterleaver::new(ldro(args.spreading_factor), args.spreading_factor);
    let hamming_dec: HammingDecoder = HammingDecoder::new();
    let header_decoder: HeaderDecoder = HeaderDecoder::new(HeaderMode::Explicit, false);
    let decoder: Decoder = Decoder::new();
    let udp_data: BlobToUdp = BlobToUdp::new("127.0.0.1:55555");
    let udp_rftap: BlobToUdp = BlobToUdp::new("127.0.0.1:55556");
    connect!(fg,
        transmitter > frame_sync > fft_demod > gray_mapping > deinterleaver > hamming_dec > header_decoder;
        header_decoder.frame_info | frame_info.frame_sync;
        header_decoder | decoder;
        decoder.crc_check | payload_crc_result.frame_sync;
        decoder.out | udp_data;
        decoder.rftap | udp_rftap;
    );

    let transmitter = transmitter.into();

    // ==============================================================
    // Send Frames
    // ==============================================================
    let rt = Runtime::new();
    let (_fg, mut handle) = rt.start_sync(fg)?;
    rt.block_on(async move {
        let mut counter: usize = 0;
        loop {
            let payload = format!("hello world! {counter:02}");
            handle
                .call(transmitter, "msg", Pmt::String(payload))
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
