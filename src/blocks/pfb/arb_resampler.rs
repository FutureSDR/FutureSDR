use futuredsp::FirFilter;
use futuredsp::prelude::*;
use num_complex::Complex32;
use std::cmp::min;

use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Polyphase Arbitrary Rate Resampler
pub struct PfbArbResampler {
    rate: f32,
    /* The number of filters is specified by the user as the
       filter size; this is also the interpolation rate of the
       filter. We use it and the rate provided to determine the
       decimation rate. This acts as a rational resampler. The
       flt_rate is calculated as the residual between the integer
       decimation rate and the real decimation rate and will be
       used to determine to interpolation point of the resampling
       process.
    */
    num_filters: usize,
    n_taps_per_filter: usize,
    fir_filters: Vec<FirFilter<Complex32, Complex32, Vec<f32>>>,
    diff_filters: Vec<FirFilter<Complex32, Complex32, Vec<f32>>>,
    filter_index: usize,
    dec_rate: usize,
    // This calculation finds the phase offset induced by the
    // arbitrary resampling. It's based on which filter arm we are
    // at the filter's group delay plus the fractional offset
    // between the samples. Calculated here based on the rotation
    // around nfilts starting at start_filter.
    accum: f32, // accumulator; holds fractional part of sample
    // residual rate for the linear interpolation
    flt_rate: f32,
    buff: [Complex32; 2],
}

impl PfbArbResampler {
    fn taps_per_filter(num_taps: usize, num_filts: usize) -> usize {
        (num_taps as f32 / num_filts as f32).ceil() as usize
    }

    fn create_diff_taps(taps: &[f32]) -> Vec<f32> {
        let diff_filter: [f32; 2] = [-1., 1.];
        let mut diff_taps: Vec<f32> = vec![0.; taps.len()];
        for i in 0..taps.len() - 1 {
            for j in 0..2 {
                diff_taps[i] += diff_filter[j] * taps[i + j];
            }
        }
        diff_taps[taps.len() - 1] = 0.;
        diff_taps
    }

    fn create_taps(
        taps: &[f32],
        num_filters: usize,
    ) -> Vec<FirFilter<Complex32, Complex32, Vec<f32>>> {
        let taps_per_filter = Self::taps_per_filter(taps.len(), num_filters);
        // Make a vector of the taps plus fill it out with 0's to fill
        // each polyphase filter with exactly d_taps_per_filter
        let mut fir_filters = vec![];
        for i in 0..num_filters {
            let mut taps_tmp: Vec<f32> = taps[i..].iter().step_by(num_filters).copied().collect();
            if taps_tmp.len() < taps_per_filter {
                taps_tmp.push(0.);
            }
            fir_filters.push(FirFilter::<Complex32, Complex32, _>::new(taps_tmp));
        }
        fir_filters
    }

    #[allow(clippy::type_complexity)]
    fn build_filterbank(
        taps: &[f32],
        num_filters: usize,
    ) -> (
        Vec<FirFilter<Complex32, Complex32, Vec<f32>>>,
        Vec<FirFilter<Complex32, Complex32, Vec<f32>>>,
    ) {
        let diff_taps = Self::create_diff_taps(taps);
        let filters = Self::create_taps(taps, num_filters);
        let diff_filters = Self::create_taps(&diff_taps, num_filters);
        (filters, diff_filters)
    }

    /// Create Arbitrary Rate Resampler.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(rate: f32, taps: &[f32], num_filters: usize) -> TypedBlock<Self> {
        let (filters, diff_filters) = Self::build_filterbank(taps, num_filters);
        let n_taps_per_filter = Self::taps_per_filter(taps.len(), num_filters);

        let dec_rate: f32 = (num_filters as f32 / rate).floor();
        let flt_rate = (num_filters as f32 / rate) - dec_rate;

        let starting_filter = (taps.len() / 2) % num_filters;

        TypedBlock::new(
            BlockMetaBuilder::new("PfbArbResampler").build(),
            StreamIoBuilder::new()
                .add_input::<Complex32>("in")
                .add_output::<Complex32>("out")
                .build(),
            MessageIoBuilder::new().build(),
            PfbArbResampler {
                rate,
                num_filters,
                n_taps_per_filter,
                fir_filters: filters,
                diff_filters,
                filter_index: starting_filter,
                dec_rate: dec_rate as usize,
                accum: 0.0,
                flt_rate,
                buff: [Complex32::new(0., 0.); 2],
            },
        )
    }
}

#[async_trait]
impl Kernel for PfbArbResampler {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let input = sio.input(0).slice::<Complex32>();
        let ninput_items = input
            .len()
            .saturating_sub(self.rate as usize + self.n_taps_per_filter - 1);
        let out = sio.output(0).slice::<Complex32>();
        let noutput_items = out.len();
        let nitem_to_process = min(ninput_items, (noutput_items as f32 / self.rate) as usize);
        if nitem_to_process > 0 {
            let mut i_in: usize = 0;
            let mut i_out: usize = 0;

            while i_in < nitem_to_process {
                // start j by wrapping around mod the number of channels
                while self.filter_index < self.num_filters {
                    // Take the current filter and derivative filter output
                    self.fir_filters[self.filter_index].filter(
                        &input[i_in..i_in + self.n_taps_per_filter],
                        &mut self.buff[0..1],
                    );
                    self.diff_filters[self.filter_index].filter(
                        &input[i_in..i_in + self.n_taps_per_filter],
                        &mut self.buff[1..2],
                    );

                    out[i_out] = self.buff[0] + self.buff[1] * self.accum; // linearly interpolate between samples
                    i_out += 1;

                    // Adjust accumulator and index into filterbank
                    self.accum += self.flt_rate;
                    self.filter_index += self.dec_rate + self.accum.floor() as usize;
                    self.accum %= 1.0;
                }
                i_in += self.filter_index / self.num_filters;
                self.filter_index %= self.num_filters;
            }

            sio.input(0).consume(i_in);
            sio.output(0).produce(i_out);
        }
        if ninput_items - nitem_to_process < self.n_taps_per_filter && sio.input(0).finished() {
            io.finished = true;
        }
        Ok(())
    }
}
