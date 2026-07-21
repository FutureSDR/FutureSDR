use anyhow::Result;
use anyhow::bail;
use futuredsp::firdes::remez;

use crate::ble_protocol;

pub(crate) const BLE_CHANNEL_SPACING_HZ: f64 = 2.0e6;
const MULTI_CHANNEL_GUARD_BINS: usize = 1;

pub(crate) fn pfb_channelizer_taps(pfb_channels: usize) -> Vec<f32> {
    let transition_bw = 0.5;
    remez::low_pass(
        1.0,
        pfb_channels,
        0.5 - transition_bw / 2.0,
        0.5 + transition_bw / 2.0,
        0.1,
        100.0,
        None,
    )
    .into_iter()
    .map(|tap| tap as f32)
    .collect()
}

pub(crate) fn pfb_channels_from_sample_rate(sample_rate: f64) -> Result<usize> {
    let channels = sample_rate / BLE_CHANNEL_SPACING_HZ;
    let rounded = channels.round();

    if (channels - rounded).abs() > 1e-6 {
        bail!(
            "multi-channel sample rate must be an integer multiple of 2 MHz, got {:.3} MS/s",
            sample_rate / 1.0e6
        );
    }

    let channels = rounded as usize;
    if channels <= 2 {
        bail!("multi-channel mode needs at least 4 MHz sample rate");
    }

    Ok(channels)
}

pub(crate) fn center_frequency_for_channels(channels: &[u8]) -> Result<f64> {
    let frequencies_mhz: Vec<i32> = channels
        .iter()
        .map(|&channel| {
            ble_protocol::frequency_hz_from_channel_index(channel)
                .map(|freq| (freq / BLE_CHANNEL_SPACING_HZ).round() as i32)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let min_bin = frequencies_mhz.iter().min().copied().unwrap_or(1201);
    let max_bin = frequencies_mhz.iter().max().copied().unwrap_or(min_bin);
    let center_bin = min_bin + (max_bin - min_bin + 1) / 2;

    Ok(center_bin as f64 * BLE_CHANNEL_SPACING_HZ)
}

pub(crate) fn auto_multi_channel_sample_rate(
    channels: &[u8],
    center_frequency: f64,
) -> Result<f64> {
    let max_offset_bins = channels
        .iter()
        .map(|&channel| {
            ble_protocol::frequency_hz_from_channel_index(channel).map(|freq| {
                ((freq - center_frequency) / BLE_CHANNEL_SPACING_HZ)
                    .round()
                    .abs() as usize
            })
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max()
        .unwrap_or(0);

    let pfb_channels = ((max_offset_bins + MULTI_CHANNEL_GUARD_BINS) * 2).max(4);
    Ok(pfb_channels as f64 * BLE_CHANNEL_SPACING_HZ)
}

pub(crate) fn pfb_port_for_frequency(
    frequency: f64,
    center_frequency: f64,
    pfb_channels: usize,
) -> Result<usize> {
    let offset_bins = (frequency - center_frequency) / BLE_CHANNEL_SPACING_HZ;
    let rounded = offset_bins.round();

    if (offset_bins - rounded).abs() > 1e-6 {
        bail!(
            "frequency {:.3} MHz does not align with the 2 MHz PFB grid around center {:.3} MHz",
            frequency / 1.0e6,
            center_frequency / 1.0e6,
        );
    }

    let offset_bins = rounded as isize;
    let half = (pfb_channels / 2) as isize;
    if offset_bins.abs() >= half {
        bail!(
            "frequency {:.3} MHz is outside the usable PFB span for center {:.3} MHz and {} channels",
            frequency / 1.0e6,
            center_frequency / 1.0e6,
            pfb_channels,
        );
    }

    if offset_bins >= 0 {
        Ok(offset_bins as usize)
    } else {
        Ok(pfb_channels - (-offset_bins as usize))
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use futuresdr::blocks::PfbChannelizer;
    use futuresdr::blocks::VectorSink;
    use futuresdr::blocks::VectorSource;
    use futuresdr::num_complex::Complex32;
    use futuresdr::prelude::*;
    use futuresdr::runtime::dev::DefaultCpuReader;
    use futuresdr::runtime::dev::DefaultCpuWriter;

    use super::*;

    #[test]
    fn multi_channel_center_keeps_ble_grid_alignment() {
        let center = center_frequency_for_channels(&[37, 38]).unwrap();

        assert_eq!(center, 2.414e9);
    }

    #[test]
    fn pfb_ports_map_positive_and_negative_offsets() {
        let center = 2.414e9;

        assert_eq!(
            pfb_port_for_frequency(
                ble_protocol::frequency_hz_from_channel_index(37).unwrap(),
                center,
                16
            )
            .unwrap(),
            10
        );
        assert_eq!(
            pfb_port_for_frequency(
                ble_protocol::frequency_hz_from_channel_index(38).unwrap(),
                center,
                16
            )
            .unwrap(),
            6
        );
    }

    #[test]
    fn auto_sample_rate_leaves_guard_bins() {
        let center = center_frequency_for_channels(&[37, 38]).unwrap();

        assert_eq!(
            auto_multi_channel_sample_rate(&[37, 38], center).unwrap(),
            28.0e6
        );
    }

    #[test]
    fn adjacent_advertising_and_data_channel_fit_in_lower_rate() {
        let center = center_frequency_for_channels(&[37, 0]).unwrap();

        assert_eq!(center, 2.404e9);
        assert_eq!(
            auto_multi_channel_sample_rate(&[37, 0], center).unwrap(),
            8.0e6
        );
        assert_eq!(
            pfb_port_for_frequency(
                ble_protocol::frequency_hz_from_channel_index(37).unwrap(),
                center,
                4
            )
            .unwrap(),
            3
        );
        assert_eq!(
            pfb_port_for_frequency(
                ble_protocol::frequency_hz_from_channel_index(0).unwrap(),
                center,
                4
            )
            .unwrap(),
            0
        );
    }

    #[test]
    fn pfb_routes_synthetic_ble_channel_tones() -> Result<()> {
        let pfb_channels = 14;
        let sample_count = 16_384;
        let taps = pfb_channelizer_taps(pfb_channels);
        let center = center_frequency_for_channels(&[37, 38])?;

        let ch37_port = pfb_port_for_frequency(
            ble_protocol::frequency_hz_from_channel_index(37)?,
            center,
            pfb_channels,
        )?;
        let ch38_port = pfb_port_for_frequency(
            ble_protocol::frequency_hz_from_channel_index(38)?,
            center,
            pfb_channels,
        )?;

        let ch37_powers = pfb_output_powers_db(
            tone_at_pfb_bin(-6, pfb_channels, sample_count),
            pfb_channels,
            &taps,
        )?;
        let ch38_powers = pfb_output_powers_db(
            tone_at_pfb_bin(6, pfb_channels, sample_count),
            pfb_channels,
            &taps,
        )?;

        assert_eq!(strongest_output(&ch37_powers), ch37_port);
        assert_eq!(strongest_output(&ch38_powers), ch38_port);

        assert!(
            neighbor_leakage_db(&ch37_powers, ch37_port) < -30.0,
            "ch37 neighbor leakage {:.1} dB, powers {:?}",
            neighbor_leakage_db(&ch37_powers, ch37_port),
            ch37_powers
        );
        assert!(
            neighbor_leakage_db(&ch38_powers, ch38_port) < -30.0,
            "ch38 neighbor leakage {:.1} dB, powers {:?}",
            neighbor_leakage_db(&ch38_powers, ch38_port),
            ch38_powers
        );

        Ok(())
    }

    fn tone_at_pfb_bin(bin: isize, pfb_channels: usize, sample_count: usize) -> Vec<Complex32> {
        let phase_step = 2.0 * std::f64::consts::PI * bin as f64 / pfb_channels as f64;
        (0..sample_count)
            .map(|index| {
                let phase = phase_step * index as f64;
                Complex32::new(phase.cos() as f32, phase.sin() as f32)
            })
            .collect()
    }

    fn pfb_output_powers_db(
        input: Vec<Complex32>,
        pfb_channels: usize,
        taps: &[f32],
    ) -> Result<Vec<f64>> {
        let mut fg = Flowgraph::new();
        let src = fg.add(VectorSource::<Complex32>::new(input));
        let channelizer = fg.add(PfbChannelizer::<
            DefaultCpuReader<Complex32>,
            DefaultCpuWriter<Complex32>,
        >::new(pfb_channels, taps, 1.0));
        fg.stream_dyn(src, "output", channelizer, "input")?;

        let mut sinks = Vec::with_capacity(pfb_channels);
        for port in 0..pfb_channels {
            let sink = fg.add(VectorSink::<Complex32>::new(2048));
            fg.stream_dyn(channelizer, format!("outputs[{port}]"), sink, "input")?;
            sinks.push(sink);
        }

        let fg = Runtime::new().run(fg)?;
        let powers = sinks
            .iter()
            .map(|sink| {
                let sink = fg.block(sink)?;
                Ok(mean_power_db(sink.items()))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(powers)
    }

    fn mean_power_db(samples: &[Complex32]) -> f64 {
        let skip = samples.len().min(16);
        let samples = &samples[skip..];
        let mean_power = samples
            .iter()
            .map(|sample| sample.norm_sqr() as f64)
            .sum::<f64>()
            / samples.len().max(1) as f64;

        10.0 * mean_power.max(1.0e-20).log10()
    }

    fn strongest_output(powers_db: &[f64]) -> usize {
        powers_db
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(index, _)| index)
            .unwrap()
    }

    fn neighbor_leakage_db(powers_db: &[f64], main_port: usize) -> f64 {
        let main_power = powers_db[main_port];
        let lower = powers_db[(main_port + powers_db.len() - 1) % powers_db.len()] - main_power;
        let upper = powers_db[(main_port + 1) % powers_db.len()] - main_power;
        lower.max(upper)
    }
}
