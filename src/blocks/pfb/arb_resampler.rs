/*
 * Derived from the liquid-dsp project.
 * Original copyright and license:
 *
 * Copyright (c) 2007 - 2024 Joseph Gaeddert
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */

use std::cmp::min;

use num_complex::Complex32;

use futuredsp::FirFilter;
use futuredsp::prelude::*;
use futuresdr::prelude::*;

use super::utilities::partition_filter_taps;
use super::window_buffer::WindowBuffer;

enum ResampState {
    Interpolate,
    Boundary,
}

struct State {
    num_filters: usize,
    fir_filters: Vec<FirFilter<Complex32, Complex32, Vec<f32>>>,
    window_buf: WindowBuffer,
    rate: f32,
    delay: f32,
    buff: [Complex32; 2], // [y0, y1]
    state: ResampState,
    tau: f32,          // accumulated timing phase
    bf: f32,           // soft-valued filterbank index
    base_index: usize, // base filterbank index
    mu: f32,           // fractional filterbank interpolation value
}

/// Polyphase Arbitrary Rate Resampler
#[derive(Block)]
pub struct PfbArbResampler<
    I: CpuBufferReader<Item = Complex32> = DefaultCpuReader<Complex32>,
    O: CpuBufferWriter<Item = Complex32> = DefaultCpuWriter<Complex32>,
> {
    s: State,
    #[input]
    input: I,
    #[output]
    output: O,
}

impl<I, O> PfbArbResampler<I, O>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    /// Create Arbitrary Rate Resampler.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(rate: f32, taps: &[f32], num_filters: usize) -> Self {
        // validate input
        assert!(
            rate > 0.,
            "PfbArbResampler: resampling rate must be greater than zero"
        );
        assert!(
            taps.len() >= num_filters,
            "PfbArbResampler: prototype filter length must be at least num_filters"
        );
        assert_ne!(
            num_filters, 0,
            "PfbArbResampler: number of filter banks must be greater than zero"
        );

        let (partitioned_filters, filter_length) = partition_filter_taps(taps, num_filters);

        let mut output = O::default();
        output.set_min_items(rate.ceil() as usize);

        Self {
            s: State {
                num_filters,
                fir_filters: partitioned_filters,
                window_buf: WindowBuffer::new(filter_length, false),
                rate,
                delay: 1.0 / rate,
                buff: [Complex32::new(0., 0.); 2],
                state: ResampState::Interpolate,
                tau: 0.0,
                bf: 0.0,
                base_index: 0,
                mu: 0.0,
            },
            input: I::default(),
            output,
        }
    }
}

impl State {
    /// update timing state; increment output timing stride and quantize filterbank indices
    fn update_timing_state(&mut self) {
        // update high-resolution timing phase
        self.tau += self.delay;
        // convert to high-resolution filterbank index
        self.bf = self.tau * self.num_filters as f32;
        // split into integer filterbank index and fractional interpolation
        self.base_index = self.bf.floor() as usize; // base index
        self.mu = self.bf - self.base_index as f32; // fractional index
    }

    fn consume_single(&mut self, sample: Complex32, out_buf: &mut [Complex32]) -> usize {
        self.window_buf.push(sample);
        let mut produced: usize = 0;
        while self.base_index < self.num_filters {
            match self.state {
                ResampState::Boundary => {
                    // compute filterbank output
                    self.fir_filters[0]
                        .filter(self.window_buf.get_as_slice(), &mut self.buff[1..2]);
                    // interpolate
                    out_buf[produced] = (1.0 - self.mu) * self.buff[0] + self.mu * self.buff[1];
                    produced += 1;
                    self.update_timing_state();
                    self.state = ResampState::Interpolate;
                }
                ResampState::Interpolate => {
                    // compute output at base index
                    self.fir_filters[self.base_index]
                        .filter(self.window_buf.get_as_slice(), &mut self.buff[0..1]);
                    // check to see if base index is last filter in the bank, in
                    // which case the resampler needs an additional input sample
                    // to finish the linear interpolation process
                    if self.base_index == self.num_filters - 1 {
                        // last filter: need additional input sample
                        self.state = ResampState::Boundary;

                        // set index to indicate new sample is needed
                        self.base_index = self.num_filters;
                    } else {
                        // do not need additional input sample; compute
                        // output at incremented base index
                        self.fir_filters[self.base_index + 1]
                            .filter(self.window_buf.get_as_slice(), &mut self.buff[1..2]);
                        // perform linear interpolation between filterbank outputs
                        out_buf[produced] = (1.0 - self.mu) * self.buff[0] + self.mu * self.buff[1];
                        produced += 1;
                        self.update_timing_state();
                    }
                }
            }
        }
        // decrement timing phase by one sample
        self.tau -= 1.0;
        self.bf -= self.num_filters as f32;
        self.base_index -= self.num_filters;
        produced
    }
}

#[doc(hidden)]
impl Kernel for PfbArbResampler {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let input = self.input.slice();
        let ninput_items = input.len();
        // fill filter history
        if !self.s.window_buf.filled() {
            let mut consumed = 0;
            while !self.s.window_buf.filled() && consumed < ninput_items {
                self.s.window_buf.push(input[consumed]);
                consumed += 1;
            }
            self.input.consume(consumed);
            if ninput_items - consumed > 0 {
                io.call_again = true;
            } else if self.input.finished() {
                io.finished = true;
            }
            return Ok(());
        }
        let out = self.output.slice();
        let noutput_items = out.len();
        let nitem_to_process = min(ninput_items, (noutput_items as f32 / self.s.rate) as usize);
        if nitem_to_process > 0 {
            let mut produced: usize = 0;
            for sample in input.iter().take(nitem_to_process) {
                produced += self.s.consume_single(*sample, &mut out[produced..])
            }
            self.input.consume(nitem_to_process);
            self.output.produce(produced);
        }
        if ninput_items - nitem_to_process == 0 && self.input.finished() {
            io.finished = true;
        }
        Ok(())
    }
}
