use clap::Parser;
use futuresdr::anyhow::Result;
use futuresdr::async_io::Timer;
use futuresdr::blocks::seify::SinkBuilder;
use futuresdr::macros::connect;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;
use futuresdr::tracing::info;
use std::time::Duration;

use lora::utilities::Bandwidth;
use lora::utilities::Channel;
use lora::utilities::CodeRate;
use lora::utilities::SpreadingFactor;
use lora::Transmitter;

#[derive(Parser, Debug)]
struct Args {
    /// TX Antenna
    #[clap(long)]
    antenna: Option<String>,
    /// Seify Device Args
    #[clap(long)]
    args: Option<String>,
    /// TX Gain
    #[clap(long, default_value_t = 50.0)]
    gain: f64,
    /// Oversampling Factor
    #[clap(long, default_value_t = 4)]
    oversampling: usize,
    /// Center Frequency
    #[clap(long, value_enum, default_value_t = Channel::EU868_1)]
    channel: Channel,
    /// Send periodic messages for testing
    #[clap(long, default_value_t = 2.0)]
    tx_interval: f32,
    /// Spreading Factor
    #[clap(long, value_enum, default_value_t = SpreadingFactor::SF7)]
    spreading_factor: SpreadingFactor,
    /// Sync Word
    #[clap(long, default_value_t = 0x0816)]
    sync_word: usize,
    /// LoRa Bandwidth
    #[clap(long, value_enum, default_value_t = Bandwidth::BW125)]
    bandwidth: Bandwidth,
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

    let mut sink = SinkBuilder::new()
        .sample_rate((Into::<usize>::into(args.bandwidth) * args.oversampling) as f64)
        .frequency(args.channel.into())
        .gain(args.gain);
    if let Some(a) = args.antenna {
        sink = sink.antenna(a);
    }
    if let Some(a) = args.args {
        sink = sink.args(a)?;
    }
    let sink = sink.build().unwrap();

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
    let fg_tx_port = transmitter
        .message_input_name_to_id("msg")
        .expect("No message_in port found!");

    connect!(fg, transmitter > sink);

    let rt = Runtime::new();

    let (_fg, mut handle) = rt.start_sync(fg);
    rt.block_on(async move {
        let mut counter: usize = 0;
        loop {
            let payload = format!("hello world! {:02}", counter).to_string();
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
