use std::path::PathBuf;

use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use clap::ValueEnum;

use crate::ble_protocol;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum DecoderMode {
    Continuous,
    Burst,
}

#[derive(Parser, Debug)]
#[command(version)]
pub(crate) struct Args {
    #[arg(short, long, default_value = "")]
    pub(crate) args: String,
    #[arg(short, long, value_delimiter = ',', default_value = "37")]
    pub(crate) channels: Vec<u8>,
    #[arg(short, long, default_value_t = 2.0e6)]
    pub(crate) sample_rate: f64,
    #[arg(short, long, default_value_t = 30.0)]
    pub(crate) gain: f64,
    #[arg(long)]
    pub(crate) antenna: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) decoder: Option<DecoderMode>,
    #[arg(long, default_value_t = false)]
    pub(crate) follow_connections: bool,
    #[arg(long, allow_hyphen_values = true)]
    pub(crate) squelch_db: Option<f32>,
    #[arg(long, default_value_t = 13.0)]
    pub(crate) squelch_margin_db: f32,
    #[arg(long, default_value_t = false)]
    pub(crate) quiet: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) wireshark: bool,
    #[arg(long, default_value = "127.0.0.1:55556")]
    pub(crate) wireshark_addr: String,
    #[arg(long, default_value_t = false)]
    pub(crate) simulate: bool,
    #[arg(long, default_value_t = 1)]
    pub(crate) simulate_packets: usize,
    #[arg(long)]
    pub(crate) iq_file: Option<PathBuf>,
    #[arg(long)]
    pub(crate) duration: Option<f64>,
}

impl Args {
    pub(crate) fn is_multi_channel(&self) -> bool {
        let mut channels = self.channels.clone();
        channels.sort_unstable();
        channels.dedup();
        channels.len() > 1
    }

    pub(crate) fn decoder_mode(&self) -> DecoderMode {
        self.decoder.unwrap_or(if self.is_multi_channel() {
            DecoderMode::Burst
        } else {
            DecoderMode::Continuous
        })
    }
}

pub(crate) fn validate(args: &Args) -> Result<()> {
    if args.simulate && args.iq_file.is_some() {
        bail!("--simulate and --iq-file cannot be used together");
    }
    if args.follow_connections && args.decoder_mode() != DecoderMode::Burst {
        bail!("--follow-connections requires --decoder burst");
    }
    if args.follow_connections && !args.is_multi_channel() {
        bail!("--follow-connections requires at least two --channels values");
    }
    if !args.sample_rate.is_finite() || args.sample_rate <= 0.0 {
        bail!("--sample-rate must be a positive finite value");
    }
    if !args.squelch_margin_db.is_finite() || args.squelch_margin_db <= 0.0 {
        bail!("--squelch-margin-db must be a positive finite value");
    }
    if args.squelch_db.is_some_and(|value| !value.is_finite()) {
        bail!("--squelch-db must be finite");
    }

    Ok(())
}

pub(crate) fn selected_channels(args: &Args) -> Result<Vec<u8>> {
    let mut channels = args.channels.clone();

    channels.sort_unstable();
    channels.dedup();

    for &channel in &channels {
        ble_protocol::frequency_hz_from_channel_index(channel)?;
    }

    Ok(channels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_channel_defaults_to_continuous_decoder() {
        let args = Args::try_parse_from(["sniffer"]).unwrap();

        assert!(!args.is_multi_channel());
        assert_eq!(args.decoder_mode(), DecoderMode::Continuous);
        assert_eq!(selected_channels(&args).unwrap(), vec![37]);
    }

    #[test]
    fn one_explicit_channel_selects_single_channel_mode() {
        let args = Args::try_parse_from(["sniffer", "--channels", "38"]).unwrap();

        assert!(!args.is_multi_channel());
        assert_eq!(args.decoder_mode(), DecoderMode::Continuous);
        assert_eq!(selected_channels(&args).unwrap(), vec![38]);
    }

    #[test]
    fn channel_list_selects_multi_channel_burst_mode() {
        let args = Args::try_parse_from(["sniffer", "--channels", "37,38"]).unwrap();

        assert!(args.is_multi_channel());
        assert_eq!(args.decoder_mode(), DecoderMode::Burst);
        assert_eq!(selected_channels(&args).unwrap(), vec![37, 38]);
    }

    #[test]
    fn multi_channel_decoder_can_be_overridden() {
        let args =
            Args::try_parse_from(["sniffer", "--channels", "37,38", "--decoder", "continuous"])
                .unwrap();

        assert_eq!(args.decoder_mode(), DecoderMode::Continuous);
    }

    #[test]
    fn duplicate_channels_still_select_single_channel_mode() {
        let args = Args::try_parse_from(["sniffer", "--channels", "37,37"]).unwrap();

        assert!(!args.is_multi_channel());
        assert_eq!(selected_channels(&args).unwrap(), vec![37]);
    }
}
