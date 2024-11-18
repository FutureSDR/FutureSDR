use futuredsp::prelude::*;
use futuredsp::FirFilter;
use rustfft::Fft;
use rustfft::FftDirection;
use rustfft::FftPlanner;
use std::cmp::min;
use std::sync::Arc;

use crate::num_complex::Complex32;
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

/// Polyphase Synthesizer.
pub struct PfbSynthesizer {
    fir_filters: Vec<FirFilter<Complex32, Complex32, Vec<f32>>>,
    taps_per_filter: usize,
    n_channels: usize,
    fir_filter_history: Vec<Vec<Complex32>>,
    fft: Arc<dyn Fft<f32>>,
}

impl PfbSynthesizer {
    /// Create Polyphase Synthesizer.
    pub fn new(n_channels: usize, taps: &[f32]) -> TypedBlock<Self> {
        let mut channelizer = PfbSynthesizer {
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

        channelizer.set_taps(taps);

        let mut sio = StreamIoBuilder::new();
        for i in 0..n_channels {
            sio = sio.add_input::<Complex32>(format!("in{i}").as_str())
        }
        sio = sio.add_output::<Complex32>("out");

        TypedBlock::new(
            BlockMetaBuilder::new("PfbSynthesizer").build(),
            sio.build(),
            MessageIoBuilder::new().build(),
            channelizer,
        )
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

#[async_trait]
impl Kernel for PfbSynthesizer {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let n_items_available = sio
            .inputs_mut()
            .iter_mut()
            .map(|x| x.slice::<Complex32>().len())
            .min()
            .unwrap();
        let n_items_to_consume = n_items_available; // ensure we leave enough samples for "overlapping" FIR filter iterations (ref. "history" property of GNU Radio blocks)
        let n_items_to_produce = sio.output(0).slice::<Complex32>().len();
        let n_items_to_process = min(n_items_to_produce / self.n_channels, n_items_to_consume);
        let out = sio.output(0).slice::<Complex32>();

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
                    let input = &sio.input(i).slice::<Complex32>()[0..n_items_to_process];
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
                sio.input(i).consume(n_items_to_process);
            }
            sio.output(0).produce(n_items_to_process * self.n_channels);
        }
        // each iteration either depletes the available input items or the available space in the out buffer, therefore no manual call_again necessary
        // appropriately propagate flowgraph termination
        if n_items_to_consume - n_items_to_process == 0 && sio.inputs().iter().all(|x| x.finished())
        {
            io.finished = true;
        }
        Ok(())
    }
}
