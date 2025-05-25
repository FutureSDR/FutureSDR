use futuredsp::prelude::*;
use futuredsp::FirFilter;
use rustfft::Fft;
use rustfft::FftDirection;
use rustfft::FftPlanner;
use std::cmp::min;
use std::sync::Arc;

use crate::prelude::*;

/// Polyphase Synthesizer.
#[derive(Block)]
pub struct PfbSynthesizer<I = DefaultCpuReader<Complex32>, O = DefaultCpuWriter<Complex32>>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    #[input]
    input: Vec<I>,
    #[output]
    output: O,
    fir_filters: Vec<FirFilter<Complex32, Complex32, Vec<f32>>>,
    taps_per_filter: usize,
    n_channels: usize,
    fir_filter_history: Vec<Vec<Complex32>>,
    fft: Arc<dyn Fft<f32>>,
}

impl<I, O> PfbSynthesizer<I, O>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    /// Create Polyphase Synthesizer.
    pub fn new(n_channels: usize, taps: &[f32]) -> Self {
        let mut ret = Self {
            input: (0..n_channels).map(|_| I::default()).collect(),
            output: O::default(),
            fir_filters: vec![],
            taps_per_filter: 0,
            n_channels,
            fir_filter_history: vec![
                vec![
                    Complex32::new(0.0, 0.0);
                    (taps.len() as f32 / n_channels as f32).ceil() as usize
                ];
                n_channels
            ],
            fft: FftPlanner::new().plan_fft(n_channels, FftDirection::Inverse),
        };
        ret.set_taps(taps);
        ret
    }

    fn set_taps(&mut self, taps: &[f32]) {
        self.fir_filters = vec![];
        self.taps_per_filter = (taps.len() as f32 / self.n_channels as f32).ceil() as usize;
        for i in 0..self.n_channels {
            let mut taps_tmp: Vec<f32> =
                taps[i..].iter().step_by(self.n_channels).copied().collect();
            if taps_tmp.len() < self.taps_per_filter {
                taps_tmp.push(0.);
            }
            self.fir_filters
                .push(FirFilter::<Complex32, Complex32, _>::new(taps_tmp));
        }
    }
}

#[doc(hidden)]
impl<I, O> Kernel for PfbSynthesizer<I, O>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let n_items_available = self
            .input
            .iter_mut()
            .map(|x| x.slice().len())
            .min()
            .unwrap();
        let n_items_to_consume = n_items_available; // ensure we leave enough samples for "overlapping" FIR filter iterations (ref. "history" property of GNU Radio blocks)
        let out = self.output.slice();
        let n_items_to_produce = out.len();
        let n_items_to_process = min(n_items_to_produce / self.n_channels, n_items_to_consume);

        if n_items_to_process > 0 {
            let mut fft_buf: Vec<Complex32> = vec![Complex32::new(0., 0.); self.n_channels];
            let mut fir_inputs: Vec<Vec<Complex32>> =
                vec![
                    vec![Complex32::new(0.0, 0.0); n_items_to_process + self.taps_per_filter];
                    self.n_channels
                ];

            for n in 0..n_items_to_process {
                #[allow(clippy::needless_range_loop)]
                for i in 0..self.n_channels {
                    let input = &self.input[i].slice()[0..n_items_to_process];
                    fft_buf[i] = input[n];
                }
                self.fft.process(&mut fft_buf);
                for i in 0..self.n_channels {
                    fir_inputs[i][self.taps_per_filter + n] = fft_buf[i];
                }
            }

            #[allow(clippy::needless_range_loop)]
            for i in 0..self.n_channels {
                // use history
                fir_inputs[i][0..self.taps_per_filter].copy_from_slice(&self.fir_filter_history[i]);
                // update history
                self.fir_filter_history[i].copy_from_slice(&fir_inputs[i][n_items_to_process..]);
                let mut fir_output = vec![Complex32::new(0., 0.); n_items_to_process];
                let _ = self.fir_filters[i].filter(&fir_inputs[i], &mut fir_output);
                for (out_slot, &fir_out_val) in out
                    .iter_mut()
                    .skip(i)
                    .step_by(self.n_channels)
                    .zip(fir_output.iter())
                {
                    *out_slot = fir_out_val;
                }
                // for j in n_items_to_process {
                //     out[i + n_items_to_process * self.n_channels] = fir_output[j];
                // }
            }

            for i in 0..self.n_channels {
                self.input[i].consume(n_items_to_process);
            }
            self.output.produce(n_items_to_process * self.n_channels);
        }
        // each iteration either depletes the available input items or the available space in the out buffer, therefore no manual call_again necessary
        // appropriately propagate flowgraph termination
        if n_items_to_consume - n_items_to_process == 0
            && self.input.iter_mut().all(|x| x.finished())
        {
            io.finished = true;
        }
        Ok(())
    }
}
