use crate::N_SAMPLES_PER_HALF_SYM;
use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::Tag;
use futuresdr::runtime::WorkIo;

pub struct PreambleDetector {
    detection_threshold: f32,
}

impl PreambleDetector {
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
        PreambleDetector::PREAMBLE
            .into_iter()
            .rev()
            .flat_map(|n| std::iter::repeat(n).take(N_SAMPLES_PER_HALF_SYM))
            .collect()
    }

    #[allow(clippy::new_ret_no_self)]
    pub fn new(detection_threshold: f32) -> Block {
        Block::new(
            BlockMetaBuilder::new("PreambleDetector").build(),
            StreamIoBuilder::new()
                .add_input::<f32>("in_samples")
                .add_input::<f32>("in_nf")
                .add_input::<f32>("in_preamble_corr")
                .add_output::<f32>("out")
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                detection_threshold,
            },
        )
    }
}

#[async_trait]
impl Kernel for PreambleDetector {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let samples = sio.input(0).slice::<f32>();
        let nf = sio.input(1).slice::<f32>();
        let corr = sio.input(2).slice::<f32>();
        let out = sio.output(0).slice::<f32>();

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
                    sio.output(0).add_tag(
                        max_corr_idx,
                        Tag::NamedF32("preamble_start".to_string(), max_corr),
                    );
                }
            } else {
                out[num_read] = samples[num_read];
                num_read += 1;
            }
        }

        sio.input(0).consume(num_read);
        sio.input(1).consume(num_read);
        sio.input(2).consume(num_read);
        sio.output(0).produce(num_read);

        if sio.input(0).finished() || sio.input(1).finished() || sio.input(2).finished() {
            io.finished = true;
        }

        Ok(())
    }
}
