//! Frequency Xlating FIR
use futuredsp::DecimatingFirFilter;
use futuredsp::Rotator;
use futuredsp::firdes;
use futuredsp::prelude::*;

use crate::runtime::dev::prelude::*;

/// Frequency Xlating FIR filter.
///
/// Rotates a complex stream by `offset`, filters it, and decimates it.
///
/// # Stream Inputs
///
/// `input`: Complex input samples.
///
/// # Stream Outputs
///
/// `output`: Frequency-shifted and decimated complex samples.
///
/// # Usage
/// ```
/// use futuresdr::blocks::XlatingFir;
///
/// let xlating = XlatingFir::new(4, 12_000.0, 1_000_000.0);
/// ```
#[derive(Block)]
pub struct XlatingFir<I = DefaultCpuReader<Complex32>, O = DefaultCpuWriter<Complex32>>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    #[input]
    input: I,
    #[output]
    output: O,
    filter: DecimatingFirFilter<Complex32, Complex32, Vec<Complex32>>,
    rotator: Rotator,
}

impl XlatingFir<DefaultCpuReader<Complex32>, DefaultCpuWriter<Complex32>> {
    /// Create a new non-resampling FIR filter with the specified taps and default stream buffers.
    pub fn new(decimation: usize, offset: f32, sample_rate: f32) -> Self {
        assert!(decimation >= 2, "Xlating FIR: Decimation has to be >= 2");
        let transition_bw = 0.1;
        let cutoff = (0.5f64 - transition_bw - f64::EPSILON).min(1.0 / decimation as f64);
        let taps = firdes::kaiser::lowpass::<f32>(cutoff, transition_bw, 0.0001);
        Self::with_taps(taps, decimation, offset, sample_rate)
    }

    /// Create Xlating FIR block with default stream buffers.
    pub fn with_taps(taps: Vec<f32>, decimation: usize, offset: f32, sample_rate: f32) -> Self {
        Self::with_taps_and_buffers(taps, decimation, offset, sample_rate)
    }
}

impl<I, O> XlatingFir<I, O>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    /// Create a new non-resampling FIR filter with the specified taps and custom stream buffers.
    pub fn with_buffers(decimation: usize, offset: f32, sample_rate: f32) -> Self {
        assert!(decimation >= 2, "Xlating FIR: Decimation has to be >= 2");
        let transition_bw = 0.1;
        let cutoff = (0.5f64 - transition_bw - f64::EPSILON).min(1.0 / decimation as f64);
        let taps = firdes::kaiser::lowpass::<f32>(cutoff, transition_bw, 0.0001);
        Self::with_taps_and_buffers(taps, decimation, offset, sample_rate)
    }

    /// Create Xlating FIR block with custom stream buffers.
    pub fn with_taps_and_buffers(
        taps: Vec<f32>,
        decimation: usize,
        offset: f32,
        sample_rate: f32,
    ) -> Self {
        assert!(decimation != 0);

        let mut bpf_taps = Vec::new();
        for (i, tap) in taps.iter().enumerate() {
            bpf_taps.push(
                Complex32::from_polar(1.0, i as f32 * std::f32::consts::TAU * offset / sample_rate)
                    * tap,
            );
        }

        Self {
            input: I::default(),
            output: O::default(),
            filter: DecimatingFirFilter::new(decimation, bpf_taps),
            rotator: Rotator::new(
                -std::f32::consts::TAU * offset * decimation as f32 / sample_rate,
            ),
        }
    }
}

#[doc(hidden)]
impl<I, O> Kernel for XlatingFir<I, O>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = self.input.slice();
        let o = self.output.slice();

        let (consumed, produced, status) = self.filter.filter(i, o);

        self.rotator.rotate_inplace(&mut o[0..produced]);

        self.input.consume(consumed);
        self.output.produce(produced);

        if self.input.finished() && !matches!(status, ComputationStatus::InsufficientOutput) {
            io.finished = true;
        }

        Ok(())
    }
}
