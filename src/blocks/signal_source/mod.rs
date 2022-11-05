use crate::anyhow::Result;
use crate::num_complex::Complex32;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

mod fxpt_phase;
pub use fxpt_phase::FixedPointPhase;

mod fxpt_nco;
pub use fxpt_nco::NCO;

pub struct SignalSource<F, A>
where
    F: FnMut(FixedPointPhase) -> A + Send + 'static,
    A: Send + 'static,
{
    nco: NCO,
    phase_to_amplitude: F,
    amplitude: A,
    offset: A,
}

impl<F, A> SignalSource<F, A>
where
    F: FnMut(FixedPointPhase) -> A + Send + 'static,
    A: Copy + Send + 'static + std::ops::Mul<Output = A> + std::ops::Add<Output = A>,
{
    pub fn new(phase_to_amplitude: F, nco: NCO, amplitude: A, offset: A) -> Block {
        Block::new(
            BlockMetaBuilder::new("SignalSource").build(),
            StreamIoBuilder::new().add_output::<A>("out").build(),
            MessageIoBuilder::<Self>::new().build(),
            SignalSource {
                nco,
                phase_to_amplitude,
                amplitude,
                offset,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<F, A> Kernel for SignalSource<F, A>
where
    F: FnMut(FixedPointPhase) -> A + Send + 'static,
    A: Copy + Send + 'static + std::ops::Mul<Output = A> + std::ops::Add<Output = A>,
{
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let o = sio.output(0).slice::<A>();

        for v in o.iter_mut() {
            let a = (self.phase_to_amplitude)(self.nco.phase);
            let a = a * self.amplitude;
            let a = a + self.offset;
            *v = a;
            self.nco.step();
        }

        sio.output(0).produce(o.len());

        Ok(())
    }
}

enum WaveForm {
    Sin,
    Cos,
    Square,
}

pub struct SignalSourceBuilder<A> {
    offset: A,
    amplitude: A,
    sample_rate: f32,
    frequency: f32,
    initial_phase: f32,
    wave_form: WaveForm,
}

impl<A> SignalSourceBuilder<A> {
    pub fn offset(mut self, offset: A) -> SignalSourceBuilder<A> {
        self.offset = offset;
        self
    }

    pub fn amplitude(mut self, amplitude: A) -> SignalSourceBuilder<A> {
        self.amplitude = amplitude;
        self
    }

    pub fn initial_phase(mut self, initial_phase: f32) -> SignalSourceBuilder<A> {
        self.initial_phase = initial_phase;
        self
    }
}

impl SignalSourceBuilder<f32> {
    pub fn cos(frequency: f32, sample_rate: f32) -> SignalSourceBuilder<f32> {
        SignalSourceBuilder {
            offset: 0.0,
            amplitude: 1.0,
            sample_rate,
            frequency,
            initial_phase: 0.0,
            wave_form: WaveForm::Cos,
        }
    }

    pub fn sin(frequency: f32, sample_rate: f32) -> SignalSourceBuilder<f32> {
        SignalSourceBuilder {
            offset: 0.0,
            amplitude: 1.0,
            sample_rate,
            frequency,
            initial_phase: 0.0,
            wave_form: WaveForm::Sin,
        }
    }

    pub fn square(frequency: f32, sample_rate: f32) -> SignalSourceBuilder<f32> {
        SignalSourceBuilder {
            offset: 0.0,
            amplitude: 1.0,
            sample_rate,
            frequency,
            initial_phase: 0.0,
            wave_form: WaveForm::Square,
        }
    }

    pub fn build(self) -> Block {
        let nco = NCO::new(
            self.initial_phase,
            2.0 * core::f32::consts::PI * self.frequency / self.sample_rate,
        );
        match self.wave_form {
            WaveForm::Cos => SignalSource::new(
                |phase: FixedPointPhase| phase.cos(),
                nco,
                self.amplitude,
                self.offset,
            ),
            WaveForm::Sin => SignalSource::new(
                |phase: FixedPointPhase| phase.sin(),
                nco,
                self.amplitude,
                self.offset,
            ),
            WaveForm::Square => SignalSource::new(
                |phase: FixedPointPhase| {
                    if phase.value < 0 {
                        1.0
                    } else {
                        0.0
                    }
                },
                nco,
                self.amplitude,
                self.offset,
            ),
        }
    }
}

impl SignalSourceBuilder<Complex32> {
    pub fn cos(frequency: f32, sample_rate: f32) -> SignalSourceBuilder<Complex32> {
        SignalSourceBuilder {
            offset: Complex32::new(0.0, 0.0),
            amplitude: Complex32::new(1.0, 0.0),
            sample_rate,
            frequency,
            initial_phase: 0.0,
            wave_form: WaveForm::Cos,
        }
    }

    pub fn sin(frequency: f32, sample_rate: f32) -> SignalSourceBuilder<Complex32> {
        SignalSourceBuilder {
            offset: Complex32::new(0.0, 0.0),
            amplitude: Complex32::new(1.0, 0.0),
            sample_rate,
            frequency,
            initial_phase: 0.0,
            wave_form: WaveForm::Sin,
        }
    }

    pub fn square(frequency: f32, sample_rate: f32) -> SignalSourceBuilder<Complex32> {
        SignalSourceBuilder {
            offset: Complex32::new(0.0, 0.0),
            amplitude: Complex32::new(1.0, 0.0),
            sample_rate,
            frequency,
            initial_phase: 0.0,
            wave_form: WaveForm::Square,
        }
    }

    pub fn build(self) -> Block {
        let nco = NCO::new(
            self.initial_phase,
            2.0 * core::f32::consts::PI * self.frequency / self.sample_rate,
        );
        match self.wave_form {
            WaveForm::Cos | WaveForm::Sin => SignalSource::new(
                |phase: FixedPointPhase| Complex32::new(phase.cos(), phase.sin()),
                nco,
                self.amplitude,
                self.offset,
            ),
            WaveForm::Square => SignalSource::new(
                |phase: FixedPointPhase| {
                    let t = phase.value >> 30;
                    match t {
                        -2 => Complex32::new(1.0, 0.0),
                        -1 => Complex32::new(1.0, 1.0),
                        0 => Complex32::new(0.0, 1.0),
                        1 => Complex32::new(0.0, 0.0),
                        _ => unreachable!(),
                    }
                },
                nco,
                self.amplitude,
                self.offset,
            ),
        }
    }
}
