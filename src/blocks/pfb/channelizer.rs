use futuredsp::prelude::*;
use futuredsp::FirFilter;
use num_integer::Integer;
use rustfft::Fft;
use rustfft::FftDirection;
use rustfft::FftPlanner;
use std::cmp::min;
use std::sync::Arc;

use crate::prelude::*;

fn partition_filter_taps(
    taps: &[f32],
    n_filters: usize,
) -> (Vec<FirFilter<Complex32, Complex32, Vec<f32>>>, usize) {
    let mut fir_filters = vec![];
    let taps_per_filter = (taps.len() as f32 / n_filters as f32).ceil() as usize;
    for i in 0..n_filters {
        let mut taps_tmp: Vec<f32> = taps[i..].iter().step_by(n_filters).copied().collect();
        if taps_tmp.len() < taps_per_filter {
            taps_tmp.push(0.);
        }
        fir_filters.push(FirFilter::<Complex32, Complex32, _>::new(taps_tmp));
    }
    (fir_filters, taps_per_filter)
}

/// Polyphase Channelizer
#[derive(Block)]
pub struct PfbChannelizer<I = DefaultCpuReader<Complex32>, O = DefaultCpuWriter<Complex32>>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    #[input]
    inputs: Vec<I>,
    #[output]
    outputs: Vec<O>,
    fir_filters: Vec<FirFilter<Complex32, Complex32, Vec<f32>>>,
    taps_per_filter: usize,
    n_filters: usize,
    os_factor: f32,
    idx_lut: Vec<usize>,
    fft: Arc<dyn Fft<f32>>,
    fft_buf: Vec<Complex32>,
    rate_ratio: usize,
    num_filtering_rounds: usize,
}

impl<I, O> PfbChannelizer<I, O>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    /// Create Polyphase Channelizer.
    pub fn new(n_filters: usize, taps: &[f32], oversample_rate: f32) -> Self {
        if oversample_rate == 0. || n_filters as f32 % oversample_rate != 0. {
            panic!("pfb_channelizer: oversample rate must be N/i for i in [1, N]");
        }
        let rate_ratio = (n_filters as f32 / oversample_rate) as usize; // no rounding necessary, since condition above ensures the result is integer
        let idx_lut = (0..n_filters)
            .map(|i| n_filters - ((i + rate_ratio) % n_filters) - 1)
            .collect();
        // Calculate the number of filtering rounds to do to evenly
        // align the input vectors with the output channels
        let num_filtering_rounds = n_filters.lcm(&rate_ratio) / n_filters;
        let (fir_filters, taps_per_filter) = partition_filter_taps(taps, n_filters);

        Self {
            inputs: (0..n_filters).map(|_| I::default()).collect(),
            outputs: (0..n_filters).map(|_| O::default()).collect(),
            fir_filters,
            taps_per_filter,
            n_filters,
            os_factor: oversample_rate,
            idx_lut,
            fft: FftPlanner::new().plan_fft(n_filters, FftDirection::Inverse),
            fft_buf: vec![Complex32::new(0.0, 0.0); n_filters],
            rate_ratio,
            num_filtering_rounds,
        }
    }
}

#[doc(hidden)]
impl<I, O> Kernel for PfbChannelizer<I, O>
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
            .inputs
            .iter_mut()
            .map(|x| x.slice().len())
            .min()
            .unwrap();
        let n_items_to_consume = n_items_available.saturating_sub(self.taps_per_filter); // ensure we leave enough samples for "overlapping" FIR filter iterations (ref. "history" property of GNU Radio blocks)
        let n_items_producable = self
            .outputs
            .iter_mut()
            .map(|x| x.slice().len())
            .min()
            .unwrap();
        let n_items_to_process = min(
            (n_items_producable as f32 / self.os_factor) as usize,
            n_items_to_consume,
        );
        // consume in batches of self.rate_ratio, but ensure we are doing full iterations aligned with the number of input buffers (so as not to lose state between calls)
        let n_items_to_process =
            (n_items_to_process / self.num_filtering_rounds) * self.num_filtering_rounds;
        let n_items_to_produce = (n_items_to_process as f32 * self.os_factor) as usize;

        if n_items_to_process > 0 {
            let mut outs: Vec<&mut [Complex32]> =
                self.outputs.iter_mut().map(|x| x.slice()).collect();
            let ins: Vec<&[Complex32]> = self.inputs.iter_mut().map(|x| x.slice()).collect();
            let mut n = 1;
            let mut oo = 0;
            let mut i: isize = -1;
            while n <= n_items_to_process {
                let mut j = 0;
                i = ((i + self.rate_ratio as isize) as usize % self.n_filters) as isize;
                let last = i;
                while i >= 0 {
                    self.fir_filters[i as usize].filter(
                        &ins[j][n..n + self.taps_per_filter],
                        &mut self.fft_buf[self.idx_lut[j]..self.idx_lut[j] + 1],
                    );
                    j += 1;
                    i -= 1;
                }

                i = self.n_filters as isize - 1;
                while i > last {
                    self.fir_filters[i as usize].filter(
                        &ins[j][(n - 1)..(n + self.taps_per_filter - 1)],
                        &mut self.fft_buf[self.idx_lut[j]..self.idx_lut[j] + 1],
                    );
                    j += 1;
                    i -= 1;
                }

                if (i as usize + self.rate_ratio) >= self.n_filters {
                    n += 1;
                }

                // despin through FFT
                self.fft.process(&mut self.fft_buf);

                // Send to output channels
                #[allow(clippy::needless_range_loop)]
                for nn in 0..self.n_filters {
                    outs[nn][oo] = self.fft_buf[nn];
                }
                oo += 1;
            }
            assert_eq!(n_items_to_produce, oo);

            for i in 0..self.n_filters {
                self.inputs[i].consume(n_items_to_process);
                self.outputs[i].produce(n_items_to_produce);
            }
        }
        // each iteration either depletes the available input items or the available space in the out buffer, therefore no manual call_again necessary
        // appropriately propagate flowgraph termination
        if n_items_to_consume - n_items_to_process
            < self.taps_per_filter + self.num_filtering_rounds
            && self.inputs.iter_mut().all(|x| x.finished())
        {
            io.finished = true;
            debug!("PfbChannelizer: Terminated.")
        }
        Ok(())
    }
}
