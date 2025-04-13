//! SignalSource using Lookup Tables
mod fxpt_phase;
pub use fxpt_phase::FixedPointPhase;
mod fxpt_nco;
pub use fxpt_nco::NCO;

use std::marker::PhantomData;
use crate::prelude::*;

/// Signal Source block
#[derive(Block)]
pub struct SignalSource<A, F, O = circular::Writer<A>>
where
    A: Send + 'static,
    F: FnMut(FixedPointPhase) -> A + Send + 'static,
    O: CpuBufferWriter<Item = A>,
{
    #[output]
    output: O,
    nco: NCO,
    phase_to_amplitude: F,
    amplitude: f32,
}

impl<A, F, O> SignalSource<A, F, O>
where
    A: Copy + Send + 'static + std::ops::Mul<Output = A> + std::ops::Add<Output = A>,
    F: FnMut(FixedPointPhase) -> A + Send + 'static,
    O: CpuBufferWriter<Item = A>,
{
    /// Create SignalSource block
    pub fn new(phase_to_amplitude: F, nco: NCO, amplitude: f32) -> Self {
        Self {
            output: O::default(),
            nco,
            phase_to_amplitude,
            amplitude,
        }
    }

    /// Set amplitude
    pub fn set_amplitude(&mut self, amplitude: f32) {
        self.amplitude = amplitude;
    }
}

#[doc(hidden)]
impl<A, F, O> Kernel for SignalSource<A, F, O>
where
    A: Copy + Send + 'static + std::ops::Mul<f32, Output = A> + std::ops::Mul<Output = A> + std::ops::Add<Output = A>,
    F: FnMut(FixedPointPhase) -> A + Send + 'static,
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
            *v = a;
            self.nco.step();
        }

        self.output.produce(o_len);

        Ok(())
    }
}

/// Build a SignalSource block
pub struct SignalSourceBuilder<T, O = circular::Writer<T>>
where O: CpuBufferWriter<Item = T> {
    _t: PhantomData<T>,
    _o: PhantomData<O>,
}

impl<O> SignalSourceBuilder<f32, O>
where
    O: CpuBufferWriter<Item = f32>,
{
    /// Create cosine wave
    pub fn cos(
        frequency: f32,
        sample_rate: f32,
        amplitude: f32,
        initial_phase: f32,
    ) -> SignalSource<f32, impl FnMut(FixedPointPhase) -> f32 + Send + 'static, O> {
        let nco = NCO::new(
            initial_phase,
            2.0 * core::f32::consts::PI * frequency / sample_rate,
        );
        SignalSource::new(|phase: FixedPointPhase| phase.cos(), nco, amplitude)
    }
    /// Create sine wave
    pub fn sin(
        frequency: f32,
        sample_rate: f32,
        amplitude: f32,
        initial_phase: f32,
    ) -> SignalSource<f32, impl FnMut(FixedPointPhase) -> f32 + Send + 'static, O> {
        let nco = NCO::new(
            initial_phase,
            2.0 * core::f32::consts::PI * frequency / sample_rate,
        );
        SignalSource::new(|phase: FixedPointPhase| phase.sin(), nco, amplitude)
    }
    /// Create square wave
    pub fn square(
        frequency: f32,
        sample_rate: f32,
        amplitude: f32,
        initial_phase: f32,
    ) -> SignalSource<f32, impl FnMut(FixedPointPhase) -> f32 + Send + 'static, O> {
        let nco = NCO::new(
            initial_phase,
            2.0 * core::f32::consts::PI * frequency / sample_rate,
        );
        SignalSource::new( |phase: FixedPointPhase| {
                if phase.value < 0 {
                    1.0
                } else {
                    0.0
                }
            }, nco, amplitude)
    }
}

impl<O> SignalSourceBuilder<Complex32, O>
where
    O: CpuBufferWriter<Item = Complex32>,
{
    /// Create cosine signal
    pub fn cos(
        frequency: f32,
        sample_rate: f32,
        amplitude: f32,
        initial_phase: f32,
    ) -> SignalSource<Complex32, impl FnMut(FixedPointPhase) -> Complex32 + Send + 'static, O>
    {
        Self::sin(frequency, sample_rate, amplitude, initial_phase)
    }
    ///Create sine signal
    pub fn sin(
        frequency: f32,
        sample_rate: f32,
        amplitude: f32,
        initial_phase: f32,
    ) -> SignalSource<Complex32, impl FnMut(FixedPointPhase) -> Complex32 + Send + 'static, O>
    {
        let nco = NCO::new(
            initial_phase,
            2.0 * core::f32::consts::PI * frequency / sample_rate,
        );
        SignalSource::new(|phase: FixedPointPhase| Complex32::new(phase.cos(), phase.sin()), nco, amplitude)
    }

    /// Create square wave signal
    pub fn square(
        frequency: f32,
        sample_rate: f32,
        amplitude: f32,
        initial_phase: f32,
    ) -> SignalSource<Complex32, impl FnMut(FixedPointPhase) -> Complex32 + Send + 'static, O>
    {
        let nco = NCO::new(
            initial_phase,
            2.0 * core::f32::consts::PI * frequency / sample_rate,
        );
        SignalSource::new(
            |phase: FixedPointPhase| {
                let t = phase.value >> 30;
                match t {
                    -2 => Complex32::new(1.0, 0.0),
                    -1 => Complex32::new(1.0, 1.0),
                    0 => Complex32::new(0.0, 1.0),
                    1 => Complex32::new(0.0, 0.0),
                    _ => unreachable!(),
                }
            }, nco, amplitude)
    }
}
