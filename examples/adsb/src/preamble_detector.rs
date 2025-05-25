use crate::N_SAMPLES_PER_HALF_SYM;
use futuresdr::prelude::*;

#[derive(Block)]
pub struct PreambleDetector<
    IS = DefaultCpuReader<f32>,
    IN = DefaultCpuReader<f32>,
    IP = DefaultCpuReader<f32>,
    O = DefaultCpuWriter<f32>,
> where
    IS: CpuBufferReader<Item = f32>,
    IN: CpuBufferReader<Item = f32>,
    IP: CpuBufferReader<Item = f32>,
    O: CpuBufferWriter<Item = f32>,
{
    #[input]
    in_samples: IS,
    #[input]
    in_nf: IN,
    #[input]
    in_preamble_cor: IP,
    #[output]
    output: O,
    detection_threshold: f32,
}

impl<IS, IN, IP, O> PreambleDetector<IS, IN, IP, O>
where
    IS: CpuBufferReader<Item = f32>,
    IN: CpuBufferReader<Item = f32>,
    IP: CpuBufferReader<Item = f32>,
    O: CpuBufferWriter<Item = f32>,
{
    pub const PREAMBLE: [f32; 16] = [
        1.0f32, -1.0f32, // Symbol 1
        1.0f32, -1.0f32, // Symbol 2
        -1.0f32, -1.0f32, // ...
        -1.0f32, 1.0f32, //
        -1.0f32, 1.0f32, //
        -1.0f32, -1.0f32, //
        -1.0f32, -1.0f32, //
        -1.0f32, -1.0f32, // Symbol 8
    ];

    /// Returns taps for the preamble correlation filter
    pub fn preamble_correlator_taps() -> Vec<f32> {
        Self::PREAMBLE
            .into_iter()
            .rev()
            .flat_map(|n| std::iter::repeat_n(n, N_SAMPLES_PER_HALF_SYM))
            .collect()
    }

    pub fn new(detection_threshold: f32) -> Self {
        Self {
            in_samples: IS::default(),
            in_nf: IN::default(),
            in_preamble_cor: IP::default(),
            output: O::default(),
            detection_threshold,
        }
    }
}

impl Kernel for PreambleDetector {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let samples = self.in_samples.slice();
        let nf = self.in_nf.slice();
        let corr = self.in_preamble_cor.slice();
        let (out, mut out_tag) = self.output.slice_with_tags();

        let samples_to_read = [samples.len(), nf.len(), corr.len(), out.len()]
            .iter()
            .min()
            .copied()
            .unwrap();
        let samples_to_read = std::cmp::max(
            0,
            samples_to_read as isize - 2 * 16 * N_SAMPLES_PER_HALF_SYM as isize,
        ) as usize; // Ensure we have enough samples to find the peak
        let mut num_read = 0;
        while num_read < samples_to_read {
            if corr[num_read] > self.detection_threshold * nf[num_read] {
                // We detected a preamble. Now find the index that gives the highest correlation.
                let mut max_corr = corr[num_read] / nf[num_read];
                let mut max_corr_idx = num_read;
                for _i in 1..16 * N_SAMPLES_PER_HALF_SYM {
                    out[num_read] = samples[num_read];
                    num_read += 1;
                    // Check if we have a new highest peak
                    if corr[num_read] / nf[num_read] > max_corr {
                        max_corr = corr[num_read] / nf[num_read];
                        max_corr_idx = num_read;
                    }
                }
                // Do an extra sanity check to get rid of noise-triggered preambles.
                // This seems to filter quite well.
                // Calculate the power of each of the high half-symbols.
                let high_pwr = [0, 2, 7, 9].iter().map(|i| {
                    samples[max_corr_idx + i * N_SAMPLES_PER_HALF_SYM
                        ..max_corr_idx + (i + 1) * N_SAMPLES_PER_HALF_SYM]
                        .iter()
                        .sum::<f32>()
                });
                // Calculate the power of each of the low half-symbols.
                let low_pwr = [1, 3, 4, 5, 6, 8, 10, 11, 12, 13, 14, 15].iter().map(|i| {
                    samples[max_corr_idx + i * N_SAMPLES_PER_HALF_SYM
                        ..max_corr_idx + (i + 1) * N_SAMPLES_PER_HALF_SYM]
                        .iter()
                        .sum::<f32>()
                });
                let min_high_pwr = high_pwr.clone().reduce(f32::min).unwrap();
                let max_high_pwr = high_pwr.reduce(f32::max).unwrap();
                let max_low_pwr = low_pwr.reduce(f32::max).unwrap();
                // The minimum power of the high half-symbols should not be too far from
                // the maximum high power, and the maximum power of the low half-symbols
                // should be less than the maximum high power.
                if min_high_pwr > 0.1 * max_high_pwr && max_low_pwr < max_high_pwr {
                    // Tag preamble.
                    out_tag.add_tag(
                        max_corr_idx,
                        Tag::NamedF32("preamble_start".to_string(), max_corr),
                    );
                }
            } else {
                out[num_read] = samples[num_read];
                num_read += 1;
            }
        }

        self.in_samples.consume(num_read);
        self.in_nf.consume(num_read);
        self.in_preamble_cor.consume(num_read);
        self.output.produce(num_read);

        if self.in_samples.finished() || self.in_nf.finished() || self.in_preamble_cor.finished() {
            io.finished = true;
        }

        Ok(())
    }
}
