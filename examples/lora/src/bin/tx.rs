use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use futuresdr::runtime::Timer;
use futuresdr::blocks::seify::Builder;
use futuresdr::prelude::*;
use lora::build_lora_tx;
use lora::default_values::HAS_CRC;
use lora::default_values::PREAMBLE_LEN;
use lora::utils::Bandwidth;
use lora::utils::Channel;
use lora::utils::CodeRate;
use lora::utils::HeaderMode;
use lora::utils::HeaderModeEnumParser;
use lora::utils::LdroMode;
use lora::utils::SpreadingFactor;
use lora::utils::SynchWord;
use lora::utils::SynchWordEnumParser;
use lora::utils::sample_count;
use rand::Rng;

#[derive(Parser, Debug)]
struct Args {
    /// TX Antenna
    #[clap(long)]
    antenna: Option<String>,
    /// Seify Device Args
    #[clap(short, long)]
    args: Option<String>,
    /// TX Gain
    #[clap(short, long, default_value_t = 50.0)]
    gain: f64,
    /// Oversampling Factor
    #[clap(short, long, default_value_t = 4)]
    oversampling: usize,
    /// Channel Frequency
    #[clap(long, value_enum, default_value_t = Channel::EU868_1)]
    channel: Channel,
    /// Send periodic messages for testing
    #[clap(short, long, default_value_t = 2.0)]
    tx_interval: f32,
    /// Spreading Factor
    #[clap(short, long, value_enum, default_value_t = SpreadingFactor::SF7)]
    spreading_factor: SpreadingFactor,
    /// LoRa Sync Word
    #[clap(long, value_parser=SynchWordEnumParser, value_enum, default_value_t = SynchWord::Private)]
    sync_word: SynchWord,
    /// Sync Word
    #[clap(long, default_value_t = 16)]
    payload_len: usize,
    /// LoRa Bandwidth
    #[clap(short, long, value_enum, default_value_t)]
    bandwidth: Bandwidth,
    /// LoRa Code Rate
    #[clap(short, long, value_enum, default_value_t)]
    code_rate: CodeRate,
    /// LoRa Code Rate
    #[clap(long, value_parser=HeaderModeEnumParser, value_enum, default_value_t)]
    header_mode: HeaderMode,
}
const PAD: usize = 0;

fn main() -> Result<()> {
    let args = Args::parse();
    let ldro = LdroMode::AUTO;
    let ldro_enabled = ldro
        .resolve_if_auto(args.spreading_factor, args.bandwidth)
        .enabled();

    let mut fg = Flowgraph::new();

    let sink = Builder::new(args.args)?
        .sample_rate((Into::<usize>::into(args.bandwidth) * args.oversampling) as f64)
        .frequency(args.channel.into())
        .gain(args.gain)
        .antenna(args.antenna)
        .min_in_buffer_size(sample_count(
            // make sure the sink will not stall on large bursts
            args.spreading_factor,
            PREAMBLE_LEN,
            args.header_mode,
            args.payload_len,
            HAS_CRC,
            args.code_rate,
            args.oversampling,
            PAD,
            ldro_enabled,
        ))
        .build_sink()?;

    let transmitter = build_lora_tx(
        &mut fg,
        args.bandwidth,
        args.spreading_factor,
        args.code_rate,
        HAS_CRC,
        ldro,
        args.header_mode,
        args.oversampling,
        args.sync_word,
        Some(PREAMBLE_LEN),
        PAD,
    )?;

    connect!(fg, transmitter > inputs[0].sink);
    let transmitter: BlockId = transmitter.into();

    let rt = Runtime::new();

    let handle = rt.start_sync(fg)?.handle();
    Runtime::block_on(async move {
        let mut payload = vec![0u8; args.payload_len];
        loop {
            rand::rng().fill_bytes(payload.as_mut());
            handle
                .post(transmitter, "msg", Pmt::Blob(payload.clone()))
                .await
                .unwrap();
            info!("sending frame with payload {:02x?}", payload);
            Timer::after(Duration::from_secs_f32(args.tx_interval)).await;
        }
    });

    Ok(())
}
