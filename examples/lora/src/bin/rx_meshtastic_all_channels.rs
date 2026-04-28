use anyhow::Result;
use clap::Parser;

use futuredsp::firdes;
use futuresdr::blocks::MessagePipe;
use futuresdr::blocks::XlatingFir;
use futuresdr::blocks::seify::Builder;
use futuresdr::prelude::*;

use lora::build_lora_rx_soft_decoding;
use lora::meshtastic::MeshtasticChannel;
use lora::meshtastic::MeshtasticChannels;
use lora::utils::Bandwidth;
use lora::utils::Channel;
use lora::utils::HeaderMode;
use lora::utils::LdroMode;
use lora::utils::SpreadingFactor;
use lora::utils::SynchWord;

const OVERSAMPLING: usize = 4;

#[derive(Debug, Clone, clap::ValueEnum, Copy, Default)]
#[clap(rename_all = "SCREAMING_SNAKE_CASE")]
enum MeshtasticRegion {
    #[default]
    Eu,
    Us,
}

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
    #[clap(short, long, value_enum, default_value_t = MeshtasticRegion::Eu)]
    meshtastic_region: MeshtasticRegion,
    /// Meshtastic Channels (Format: <name>:<base64key>,<name>:<base64key>,..)
    #[clap(short, long)]
    channels: Option<String>,
}

fn main() -> Result<()> {
    futuresdr::runtime::init();
    let args = Args::parse();

    let mut channels = vec![];
    for chan in args.channels.clone().unwrap_or(String::new()).split(",") {
        let vals: Vec<&str> = chan.split(":").collect();
        if vals.len() == 2 {
            channels.push((vals[0].to_string(), vals[1].to_string()));
        }
    }
    info!("args {:?}, channel {:?}", &args, &channels);

    let (sample_rate, center_freq, configs) = match args.meshtastic_region {
        MeshtasticRegion::Eu => (
            1_000_000,
            869_300_000,
            vec![
                (
                    Bandwidth::BW250,
                    Channel::Custom(869_525_000),
                    vec![
                        (SpreadingFactor::SF7, LdroMode::DISABLE),
                        (SpreadingFactor::SF8, LdroMode::DISABLE),
                        (SpreadingFactor::SF9, LdroMode::DISABLE),
                        (SpreadingFactor::SF10, LdroMode::DISABLE),
                        (SpreadingFactor::SF11, LdroMode::DISABLE),
                    ],
                ),
                (
                    Bandwidth::BW125,
                    Channel::Custom(869_587_500),
                    vec![
                        (SpreadingFactor::SF11, LdroMode::ENABLE),
                        (SpreadingFactor::SF12, LdroMode::ENABLE),
                    ],
                ),
                (
                    Bandwidth::BW62,
                    Channel::Custom(869_492_500),
                    vec![(SpreadingFactor::SF12, LdroMode::ENABLE)],
                ),
            ],
        ),
        MeshtasticRegion::Us => (
            20_000_000,
            910_000_000,
            vec![
                (
                    Bandwidth::BW250,
                    Channel::Custom(906_875_000),
                    vec![
                        (SpreadingFactor::SF7, LdroMode::DISABLE),
                        (SpreadingFactor::SF8, LdroMode::DISABLE),
                        (SpreadingFactor::SF9, LdroMode::DISABLE),
                        (SpreadingFactor::SF10, LdroMode::DISABLE),
                        (SpreadingFactor::SF11, LdroMode::DISABLE),
                    ],
                ),
                (
                    Bandwidth::BW125,
                    Channel::Custom(904_437_500),
                    vec![
                        (SpreadingFactor::SF11, LdroMode::ENABLE),
                        (SpreadingFactor::SF12, LdroMode::ENABLE),
                    ],
                ),
                (
                    Bandwidth::BW62,
                    Channel::Custom(916_218_750),
                    vec![(SpreadingFactor::SF12, LdroMode::ENABLE)],
                ),
            ],
        ),
    };

    let mut fg = Flowgraph::new();
    let src = Builder::new(args.args)?
        .sample_rate(sample_rate as f64)
        .frequency(center_freq as f64)
        .gain(args.gain)
        .antenna(args.antenna)
        .build_source()?;

    let (tx_frame, rx_frame) = mpsc::channel::<Pmt>(100);
    let message_pipe = MessagePipe::new(tx_frame);
    connect!(fg, src; message_pipe);

    for (bandwidth, chan, chains) in configs.into_iter() {
        let decimation = sample_rate / Into::<usize>::into(bandwidth) / OVERSAMPLING;
        let cutoff = Into::<f64>::into(bandwidth) / 2.0 / sample_rate as f64;
        let transition_bw = cutoff;
        let taps = firdes::kaiser::lowpass(cutoff, transition_bw, 0.05);
        let decimation = XlatingFir::with_taps(
            taps,
            decimation,
            (Into::<u32>::into(chan) - center_freq) as f32,
            sample_rate as f32,
        );

        connect!(fg, src.outputs[0] > decimation);

        for (spreading_factor, ldro) in chains.into_iter() {
            let (frame_sync_ref, decoder_ref) = build_lora_rx_soft_decoding(
                &mut fg,
                chan,
                bandwidth,
                spreading_factor,
                HeaderMode::Explicit,
                ldro,
                Some(&[SynchWord::Meshtastic]),
                OVERSAMPLING,
                None,
                Some("header_crc_ok"),
                false,
                None,
            )?;
            connect!(fg,
                decimation > frame_sync_ref;
                decoder_ref | message_pipe;
            );
        }
    }

    let rt = Runtime::new();
    let _running = rt.start_sync(fg)?;
    rt.block_on(async move {
        let mut chans = MeshtasticChannels::new();
        chans.add_channel(MeshtasticChannel::new("", "AQ=="));
        for c in channels {
            chans.add_channel(MeshtasticChannel::new(&c.0, &c.1));
        }
        while let Some(x) = rx_frame.recv().await {
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
