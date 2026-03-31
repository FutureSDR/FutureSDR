use anyhow::Result;
use anyhow::anyhow;
use clap::Parser;

use futuresdr::blocks::BlobToUdp;
use futuresdr::blocks::seify::Builder;
use futuresdr::prelude::*;
use lora::build_lora_rx_soft_decoding;
use lora::utils::Bandwidth;
use lora::utils::Channel;
use lora::utils::ChannelEnumParser;
use lora::utils::HeaderMode;
use lora::utils::HeaderModeEnumParser;
use lora::utils::LdroMode;
use lora::utils::SpreadingFactor;
use lora::utils::SynchWord;
use lora::utils::SynchWordEnumParser;

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
    #[clap(short, long, value_enum, default_value_t)]
    bandwidth: Bandwidth,
    /// LoRa Sync Word
    #[clap(long, value_parser=SynchWordEnumParser, value_enum, default_value_t = SynchWord::Private)]
    sync_word: SynchWord,
    /// LoRa Explicit or Implicit Header
    #[clap(long, value_parser=HeaderModeEnumParser, value_enum, default_value_t)]
    header_mode: HeaderMode,
    /// Oversampling Factor
    #[clap(long, default_value_t = 4)]
    oversampling: usize,
}

fn main() -> Result<()> {
    futuresdr::runtime::init();
    let args = Args::parse();
    info!("args {:?}", &args);

    let mut fg = Flowgraph::new();

    let src = Builder::new(args.args)?
        .sample_rate((Into::<usize>::into(args.bandwidth) * args.oversampling) as f64)
        .frequency(args.channel.into())
        .gain(args.gain)
        .antenna(args.antenna)
        .build_source()?;

    let (frame_sync_ref, decoder_ref) = build_lora_rx_soft_decoding(
        &mut fg,
        args.channel,
        args.bandwidth,
        args.spreading_factor,
        HeaderMode::Explicit,
        LdroMode::AUTO,
        Some(&[args.sync_word]),
        args.oversampling,
        None,
        Some("header_crc_ok"),
        false,
        None,
    )?;
    let udp_data: BlobToUdp = BlobToUdp::new("127.0.0.1:55555");
    let udp_rftap: BlobToUdp = BlobToUdp::new("127.0.0.1:55556");
    connect!(fg,
        src.outputs[0] > frame_sync_ref;
        decoder_ref | udp_data;
        decoder_ref.rftap | udp_rftap;
    );

    if let Err(e) = Runtime::new().run(fg) {
        error!("{}", &e);
        return Err(anyhow!("{}", e));
    }
    Ok(())
}
