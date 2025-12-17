use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::seify::Builder;
use futuresdr::prelude::*;
use lora::Transmitter;
use lora::default_values::HAS_CRC;
use lora::default_values::IMPLICIT_HEADER;
use lora::default_values::PREAMBLE_LEN;
use lora::meshtastic::MeshtasticChannel;
use lora::meshtastic::MeshtasticConfig;
use lora::utils::Bandwidth;
use std::io::BufRead;
use std::io::Write;

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
    /// Meshtastic LoRa Config
    #[clap(short, long, value_enum)]
    meshtastic_config: MeshtasticConfig,
    /// meshtastic channel name
    #[clap(short, long)]
    name: String,
    /// meshtastic channel name
    #[clap(short, long, default_value = "AQ==")]
    key: String,
}
const PAD: usize = 10000;

fn main() -> Result<()> {
    let args = Args::parse();
    info!("args {:?}", &args);
    let (bandwidth, spreading_factor, code_rate, chan, ldro) = args.meshtastic_config.to_config();

    let interpolation = match bandwidth {
        Bandwidth::BW62 => 16,
        Bandwidth::BW125 => 8,
        Bandwidth::BW250 => 4,
        _ => panic!("wrong bandwidth for Meshtastic"),
    };

    let mut fg = Flowgraph::new();

    let sink = Builder::new(args.args)?
        .sample_rate(1e6)
        .frequency(Into::<f64>::into(chan))
        .gain(args.gain)
        .antenna(args.antenna)
        .build_sink()?;

    let transmitter: Transmitter = Transmitter::new(
        code_rate,
        HAS_CRC,
        spreading_factor,
        ldro,
        IMPLICIT_HEADER,
        interpolation,
        vec![16, 88],
        PREAMBLE_LEN,
        PAD,
    );

    connect!(fg, transmitter > inputs[0].sink);
    let transmitter = transmitter.into();

    let rt = Runtime::new();
    let (_fg, handle) = rt.start_sync(fg)?;

    let channel = MeshtasticChannel::new(&args.name, &args.key);
    loop {
        let msg = {
            let i = std::io::stdin().lock();
            let mut o = std::io::stdout().lock();
            write!(o, "{}: ", &args.name)?;
            o.flush()?;
            let mut iterator = i.lines();
            iterator.next().unwrap()?
        };
        let data = channel.encode(msg);
        let mut handle = handle.clone();

        rt.block_on(async move {
            handle
                .call(transmitter, "msg", Pmt::Blob(data))
                .await
                .unwrap();
            info!("sent frame");
        });
    }
}
