//! Frequency Xlating FIR
use futuredsp::DecimatingFirFilter;
use futuredsp::Rotator;
use futuredsp::firdes;
use futuredsp::prelude::*;

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

/// Frequency Xlating FIR filter.
pub struct XlatingFir {
    filter: DecimatingFirFilter<Complex32, Complex32, Vec<Complex32>>,
    rotator: Rotator,
}

impl XlatingFir {
    /// Create Xlating FIR block
    pub fn new(
        taps: Vec<f32>,
        decimation: usize,
        offset: f32,
        sample_rate: f32,
    ) -> TypedBlock<Self> {
        assert!(decimation != 0);

        let mut bpf_taps = Vec::new();
        for (i, tap) in taps.iter().enumerate() {
            bpf_taps.push(
                Complex32::from_polar(1.0, i as f32 * std::f32::consts::TAU * offset / sample_rate)
                    * tap,
            );
        }

        TypedBlock::new(
            BlockMetaBuilder::new("Fir").build(),
            StreamIoBuilder::new()
                .add_input::<Complex32>("in")
                .add_output::<Complex32>("out")
                .build(),
            MessageIoBuilder::<Self>::new().build(),
            Self {
                filter: DecimatingFirFilter::new(decimation, bpf_taps),
                rotator: Rotator::new(
                    -std::f32::consts::TAU * offset * decimation as f32 / sample_rate,
                ),
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for XlatingFir {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<Complex32>();
        let o = sio.output(0).slice::<Complex32>();

        let (consumed, produced, status) = self.filter.filter(i, o);

        let _ = self.rotator.rotate_inplace(&mut o[0..produced]);

        sio.input(0).consume(consumed);
        sio.output(0).produce(produced);

        if sio.input(0).finished() && !matches!(status, ComputationStatus::InsufficientOutput) {
            io.finished = true;
        }

        Ok(())
    }
}

/// Create a [XlatingFir] filter.
///
/// Uses `futuredsp` to pick the optimal FIR implementation for the given
/// constraints.
///
/// Note that there must be an implementation of [futuredsp::TapsAccessor] for
/// the taps object you pass in, see docs for details.
///
/// Additionally, there must be an available core (implementation of
/// [futuredsp::UnaryKernel]) available for the specified `SampleType` and
/// `TapsType`. See the [futuredsp docs](futuredsp::fir) for available
/// implementations.
///
/// # Inputs
///
/// `in`: Input
///
/// # Outputs
///
/// `out`: Output
///
/// # Usage
/// ```
/// use futuresdr::blocks::XlatingFirBuilder;
/// use futuresdr::runtime::Flowgraph;
/// use futuresdr::num_complex::Complex32;
///
/// let mut fg = Flowgraph::new();
///
/// let fir = fg.add_block(XlatingFirBuilder::new(2, 100e3, 1e6));
/// let fir = fg.add_block(XlatingFirBuilder::with_taps(vec![1.0, 2.0, 3.0], 2, 100e3, 1e6));
/// ```
pub struct XlatingFirBuilder;

impl XlatingFirBuilder {
    /// Create a new non-resampling FIR filter with the specified taps.
    pub fn new(decimation: usize, offset: f32, sample_rate: f32) -> TypedBlock<XlatingFir> {
        assert!(decimation >= 2, "Xlating FIR: Decimation has to be >= 2");
        let transition_bw = 0.1;
        let cutoff = (0.5f64 - transition_bw - f64::EPSILON).min(1.0 / decimation as f64);
        let taps = firdes::kaiser::lowpass::<f32>(cutoff, transition_bw, 0.0001);
        XlatingFirBuilder::with_taps(taps, decimation, offset, sample_rate)
    }

    /// Create a decimating FIR filter with standard low-pass taps.
    pub fn with_taps(
        taps: Vec<f32>,
        decimation: usize,
        offset: f32,
        sample_rate: f32,
    ) -> TypedBlock<XlatingFir> {
        XlatingFir::new(taps, decimation, offset, sample_rate)
    }
}
