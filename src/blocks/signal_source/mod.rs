//! SignalSource using Lookup Tables
mod fxpt_phase;
use std::marker::PhantomData;

pub use fxpt_phase::FixedPointPhase;

mod fxpt_nco;
pub use fxpt_nco::NCO;

use crate::prelude::*;

/// Signal Source block
#[derive(Block)]
pub struct SignalSource<F, A, O = circular::Writer<A>>
where
    F: FnMut(FixedPointPhase) -> A + Send + 'static,
    A: Send + 'static,
    O: CpuBufferWriter<Item = A>,
{
    #[output]
    output: O,
    nco: NCO,
    phase_to_amplitude: F,
    amplitude: A,
    offset: A,
}

impl<F, A, O> SignalSource<F, A, O>
where
    F: FnMut(FixedPointPhase) -> A + Send + 'static,
    A: Copy + Send + 'static + std::ops::Mul<Output = A> + std::ops::Add<Output = A>,
    O: CpuBufferWriter<Item = A>,
{
    /// Create SignalSource block
    pub fn new(phase_to_amplitude: F, nco: NCO, amplitude: A, offset: A) -> Self {
        Self {
            output: O::default(),
            nco,
            phase_to_amplitude,
            amplitude,
            offset,
        }
    }
}

#[doc(hidden)]
impl<F, A, O> Kernel for SignalSource<F, A, O>
where
    F: FnMut(FixedPointPhase) -> A + Send + 'static,
    A: Copy + Send + 'static + std::ops::Mul<Output = A> + std::ops::Add<Output = A>,
    O: CpuBufferWriter<Item = A>,
{
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let o = self.output.slice();
        let o_len = o.len();

        for v in o.iter_mut() {
            let a = (self.phase_to_amplitude)(self.nco.phase);
            let a = a * self.amplitude;
            let a = a + self.offset;
            *v = a;
            self.nco.step();
        }

        self.output.produce(o_len);

        Ok(())
    }
}

/// Build a SignalSource block
pub struct SignalSourceBuilder<A, F, O = circular::Writer<A>>
where
    F: FnMut(FixedPointPhase) -> A + Send + 'static,
    A: Copy + Send + 'static + std::ops::Mul<Output = A> + std::ops::Add<Output = A>,
    O: CpuBufferWriter<Item = A>,
{
    offset: A,
    amplitude: A,
    sample_rate: f32,
    frequency: f32,
    initial_phase: f32,
    phase_to_amplitude: F,
    _p: PhantomData<O>,
}

impl<A, F, O> SignalSourceBuilder<A, F, O>
where
    F: FnMut(FixedPointPhase) -> A + Send + 'static,
    A: Copy + Send + 'static + std::ops::Mul<Output = A> + std::ops::Add<Output = A>,
    O: CpuBufferWriter<Item = A>,
{
    /// Set y-offset (i.e., a DC component)
    pub fn offset(mut self, offset: A) -> SignalSourceBuilder<A, F, O> {
        self.offset = offset;
        self
    }
    /// Set amplitude
    pub fn amplitude(mut self, amplitude: A) -> SignalSourceBuilder<A, F, O> {
        self.amplitude = amplitude;
        self
    }
    /// Set initial phase
    pub fn initial_phase(mut self, initial_phase: f32) -> SignalSourceBuilder<A, F, O> {
        self.initial_phase = initial_phase;
        self
    }
}

impl<F, O> SignalSourceBuilder<f32, F, O>
where
    F: FnMut(FixedPointPhase) -> f32 + Send + 'static,
    O: CpuBufferWriter<Item = f32>,
{
    // /// Create cosine wave
    // pub fn cosf32(
    //     frequency: f32,
    //     sample_rate: f32,
    // ) -> SignalSourceBuilder<f32, impl FnMut(FixedPointPhase) -> f32 + Send + 'static, O> {
    //     SignalSourceBuilder {
    //         offset: 0.0,
    //         amplitude: 1.0,
    //         sample_rate,
    //         frequency,
    //         initial_phase: 0.0,
    //         phase_to_amplitude: |phase: FixedPointPhase| phase.cos(),
    //         _p: PhantomData,
    //     }
    // }
    /// Create sine wave
    pub fn sinf32(
        frequency: f32,
        sample_rate: f32,
    ) -> SignalSourceBuilder<f32, impl FnMut(FixedPointPhase) -> f32 + Send + 'static, O> {
        SignalSourceBuilder {
            offset: 0.0,
            amplitude: 1.0,
            sample_rate,
            frequency,
            initial_phase: 0.0,
            phase_to_amplitude: |phase: FixedPointPhase| phase.sin(),
            _p: PhantomData,
        }
    }
    // /// Create square wave
    // pub fn squaref32(
    //     frequency: f32,
    //     sample_rate: f32,
    // ) -> SignalSourceBuilder<f32, impl FnMut(FixedPointPhase) -> f32 + Send + 'static, O> {
    //     SignalSourceBuilder {
    //         offset: 0.0,
    //         amplitude: 1.0,
    //         sample_rate,
    //         frequency,
    //         initial_phase: 0.0,
    //         phase_to_amplitude: |phase: FixedPointPhase| {
    //             if phase.value < 0 {
    //                 1.0
    //             } else {
    //                 0.0
    //             }
    //         },
    //         _p: PhantomData,
    //     }
    // }
    /// Create Signal Source block
    pub fn build(self) -> SignalSource<F, f32, O> {
        let nco = NCO::new(
            self.initial_phase,
            2.0 * core::f32::consts::PI * self.frequency / self.sample_rate,
        );
        SignalSource::new(self.phase_to_amplitude, nco, self.amplitude, self.offset)
    }
}

impl<O, F> SignalSourceBuilder<Complex32, F, O>
where
    F: FnMut(FixedPointPhase) -> Complex32 + Send + 'static,
    O: CpuBufferWriter<Item = Complex32>,
{
    /// Create cosine signal
    pub fn cos(
        frequency: f32,
        sample_rate: f32,
    ) -> SignalSourceBuilder<Complex32, impl FnMut(FixedPointPhase) -> Complex32 + Send + 'static, O>
    {
        SignalSourceBuilder {
            offset: Complex32::new(0.0, 0.0),
            amplitude: Complex32::new(1.0, 0.0),
            sample_rate,
            frequency,
            initial_phase: 0.0,
            phase_to_amplitude: |phase: FixedPointPhase| Complex32::new(phase.cos(), phase.sin()),
            _p: PhantomData,
        }
    }
    ///Create sine signal
    pub fn sin(
        frequency: f32,
        sample_rate: f32,
    ) -> SignalSourceBuilder<Complex32, impl FnMut(FixedPointPhase) -> Complex32 + Send + 'static, O>
    {
        SignalSourceBuilder {
            offset: Complex32::new(0.0, 0.0),
            amplitude: Complex32::new(1.0, 0.0),
            sample_rate,
            frequency,
            initial_phase: 0.0,
            phase_to_amplitude: |phase: FixedPointPhase| Complex32::new(phase.cos(), phase.sin()),
            _p: PhantomData,
        }
    }

    /// Create square wave signal
    pub fn square(
        frequency: f32,
        sample_rate: f32,
    ) -> SignalSourceBuilder<Complex32, impl FnMut(FixedPointPhase) -> Complex32 + Send + 'static, O>
    {
        SignalSourceBuilder {
            offset: Complex32::new(0.0, 0.0),
            amplitude: Complex32::new(1.0, 0.0),
            sample_rate,
            frequency,
            initial_phase: 0.0,
            phase_to_amplitude: |phase: FixedPointPhase| {
                let t = phase.value >> 30;
                match t {
                    -2 => Complex32::new(1.0, 0.0),
                    -1 => Complex32::new(1.0, 1.0),
                    0 => Complex32::new(0.0, 1.0),
                    1 => Complex32::new(0.0, 0.0),
                    _ => unreachable!(),
                }
            },
            _p: PhantomData,
        }
    }
    /// Create Signal Source block
    pub fn build(self) -> SignalSource<F, Complex32, O> {
        let nco = NCO::new(
            self.initial_phase,
            2.0 * core::f32::consts::PI * self.frequency / self.sample_rate,
        );
        SignalSource::new(self.phase_to_amplitude, nco, self.amplitude, self.offset)
    }
}
