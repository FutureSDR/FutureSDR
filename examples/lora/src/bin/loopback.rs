use std::fmt::Debug;
use std::time::Duration;

use clap::Parser;

use futuresdr::async_io::Timer;
use futuresdr::blocks::BlobToUdp;
use futuresdr::prelude::*;

use lora::build_lora_rx_soft_decoding;
use lora::build_lora_tx;
use lora::default_values::HAS_CRC;
use lora::default_values::PREAMBLE_LEN;
use lora::utils::Bandwidth;
use lora::utils::Channel;
use lora::utils::CodeRate;
use lora::utils::HeaderMode;
use lora::utils::LdroMode;
use lora::utils::SpreadingFactor;
use lora::utils::SynchWord;
use lora::utils::SynchWordEnumParser;

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
    #[clap(long, value_enum, default_value_t)]
    bandwidth: Bandwidth,
    /// Oversampling Factor
    #[clap(long, default_value_t = 1)]
    oversampling: usize,
    /// Sync Word
    #[clap(long, value_parser=SynchWordEnumParser, value_enum, default_value_t = SynchWord::Private)]
    sync_word: SynchWord,
    /// Soft Decoding
    #[clap(long, default_value_t = false)]
    soft_decoding: bool,
    /// LoRa Code Rate
    #[clap(long, value_enum, default_value_t)]
    code_rate: CodeRate,
}

const PAD: usize = 10000;

fn main() -> Result<()> {
    let args = Args::parse();
    let ldro = LdroMode::AUTO;

    let mut fg = Flowgraph::new();

    // ==============================================================
    // TX
    // ==============================================================
    let transmitter = build_lora_tx(
        &mut fg,
        args.bandwidth,
        args.spreading_factor,
        args.code_rate,
        HAS_CRC,
        ldro,
        HeaderMode::Explicit,
        args.oversampling,
        args.sync_word,
        Some(PREAMBLE_LEN),
        PAD,
    )?;

    // ==============================================================
    // RX
    // ==============================================================
    let (frame_sync_ref, decoder_ref) = build_lora_rx_soft_decoding(
        &mut fg,
        Channel::EU868_1,
        args.bandwidth,
        args.spreading_factor,
        HeaderMode::Explicit,
        LdroMode::AUTO,
        Some(&[args.sync_word]),
        args.oversampling,
        None,
        None,
        false,
        None,
    )?;
    let udp_data: BlobToUdp = BlobToUdp::new("127.0.0.1:55555");
    let udp_rftap: BlobToUdp = BlobToUdp::new("127.0.0.1:55556");
    connect!(fg,
        transmitter > frame_sync_ref;
        decoder_ref.out | udp_data;
        decoder_ref.rftap | udp_rftap;
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
