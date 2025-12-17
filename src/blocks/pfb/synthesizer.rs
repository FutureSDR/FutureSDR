use std::sync::Arc;

use rustfft::Fft;
use rustfft::FftDirection;
use rustfft::FftPlanner;

use futuredsp::Filter;
use futuredsp::FirFilter;

use crate::prelude::*;

use super::utilities::partition_filter_taps;
use super::window_buffer::WindowBuffer;

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
    num_channels: usize,
    ifft: Arc<dyn Fft<f32>>,
    fft_buf: Vec<Complex32>,
    fir_filters: Vec<FirFilter<Complex32, Complex32, Vec<f32>>>,
    window_buf: Vec<WindowBuffer>,
    all_windows_filled: bool,
}

impl<I, O> PfbSynthesizer<I, O>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    /// Create Polyphase Synthesizer.
    pub fn new(num_channels: usize, taps: &[f32]) -> Self {
        let (partitioned_filters, filter_length) = partition_filter_taps(taps, num_channels);
        Self {
            input: (0..num_channels).map(|_| I::default()).collect(),
            output: O::default(),
            num_channels,
            ifft: FftPlanner::new().plan_fft(num_channels, FftDirection::Inverse),
            fft_buf: vec![Complex32::default(); num_channels],
            fir_filters: partitioned_filters,
            window_buf: vec![WindowBuffer::new(filter_length, false); num_channels],
            all_windows_filled: false,
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
        let out = self.output.slice();
        let inputs: Vec<&[Complex32]> = self.input.iter_mut().map(|x| x.slice()).collect();
        let n_items_to_consume = inputs.iter().map(|x| x.len()).min().unwrap();
        let n_items_to_produce = out.len();

        let mut consumed_per_channel: usize = 0;
        let mut produced: usize = 0;
        while n_items_to_consume - consumed_per_channel > 0
            && (n_items_to_produce - produced > self.num_channels || !self.all_windows_filled)
        {
            // interleave input streams, taking self.num_channels samples
            for (input, fft_input_slot) in inputs.iter().zip(self.fft_buf.iter_mut()) {
                *fft_input_slot = input[consumed_per_channel];
            }
            consumed_per_channel += 1;
            // spin through IFFT
            self.ifft.process(&mut self.fft_buf);
            for ((window, fir_filter), spun_sample) in self
                .window_buf
                .iter_mut()
                .zip(self.fir_filters.iter())
                .zip(self.fft_buf.iter())
            {
                window.push(*spun_sample);
                if window.filled() {
                    fir_filter.filter(window.get_as_slice(), &mut out[produced..produced + 1]);
                    produced += 1;
                }
            }
            if !self.all_windows_filled {
                self.all_windows_filled = self.window_buf.iter().all(|w| w.filled());
            }
        }
        if consumed_per_channel > 0 {
            for i in 0..self.num_channels {
                self.input[i].consume(consumed_per_channel);
            }
            if produced > 0 {
                self.output.produce(produced);
            }
        }
        // each iteration either depletes the available input items or the available space in the out buffer, therefore no manual call_again necessary
        // appropriately propagate flowgraph termination
        let samples_remaining_per_input: Vec<bool> = self
            .input
            .iter_mut()
            .map(|x| x.slice())
            .map(|x| x.len() - consumed_per_channel == 0)
            .collect();
        if samples_remaining_per_input
            .iter()
            .zip(self.input.iter().map(|x| x.finished()))
            .any(|(&out_of_samples, finished)| out_of_samples && finished)
        {
            io.finished = true;
        }
        Ok(())
    }
}
