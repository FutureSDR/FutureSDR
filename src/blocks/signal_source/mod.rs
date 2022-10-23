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
    _p: std::marker::PhantomData<A>,
}

impl<F, A> SignalSource<F, A>
where
    F: FnMut(FixedPointPhase) -> A + Send + 'static,
    A: Copy
        + Send
        + 'static
        + std::ops::Add
        + std::ops::Mul
        + std::ops::Mul<Output = A>
        + std::ops::Add<Output = A>,
{
    pub fn new(phase_to_amplitude: F, nco: NCO, amplitude: A, offset: A) -> Block {
        Block::new(
            BlockMetaBuilder::new("SignalSource").build(),
            StreamIoBuilder::new().add_output::<A>("out").build(),
            MessageIoBuilder::<Self>::new()
                // TODO: add freq and cmd
                .build(),
            SignalSource {
                nco,
                phase_to_amplitude,
                amplitude,
                offset,
                _p: std::marker::PhantomData,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<F, A> Kernel for SignalSource<F, A>
where
    F: FnMut(FixedPointPhase) -> A + Send + 'static,
    A: Copy
        + Send
        + 'static
        + std::ops::Mul
        + std::ops::Add
        + std::ops::Mul<Output = A>
        + std::ops::Add<Output = A>,
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

pub struct SignalSourceBuilder<A>
where
    A: Send + 'static,
{
    offset: A,
    amplitude: A,
    sampling_freq: f32,
    frequency: f32,
    initial_phase: f32,
}

impl SignalSourceBuilder<f32> {
    pub fn new() -> Self {
        SignalSourceBuilder::<f32> {
            offset: 0.0,
            amplitude: 1.0,
            sampling_freq: 48_000.0,
            frequency: 440.0,
            initial_phase: 0.0,
        }
    }

    pub fn cosine_wave(&self) -> Block {
        SignalSource::new(
            move |phase: FixedPointPhase| phase.cos(),
            self.nco(),
            self.amplitude,
            self.offset,
        )
    }

    pub fn sine_wave(&self) -> Block {
        SignalSource::new(
            move |phase: FixedPointPhase| phase.sin(),
            self.nco(),
            self.amplitude,
            self.offset,
        )
    }

    pub fn square_wave(&self) -> Block {
        SignalSource::new(
            move |phase: FixedPointPhase| {
                if phase.value < 0i32 {
                    1.0f32
                } else {
                    0.0f32
                }
            },
            self.nco(),
            self.amplitude,
            self.offset,
        )
    }
}

impl SignalSourceBuilder<Complex32> {
    pub fn new() -> Self {
        SignalSourceBuilder::<Complex32> {
            offset: Complex32::new(0.0, 0.0),
            amplitude: Complex32::new(1.0, 0.0),
            sampling_freq: 48_000.0,
            frequency: 440.0,
            initial_phase: 0.0,
        }
    }
}

impl<A> SignalSourceBuilder<A>
where
    A: Copy
        + Send
        + 'static
        + std::ops::Add
        + std::ops::Mul
        + std::ops::Mul<Output = A>
        + std::ops::Add<Output = A>,
{
    pub fn for_sampling_rate(mut self, sample_rate: f32) -> SignalSourceBuilder<A> {
        self.sampling_freq = sample_rate;
        self
    }

    pub fn with_frequency(mut self, frequency: f32) -> SignalSourceBuilder<A> {
        self.frequency = frequency;
        self
    }

    pub fn with_offset(mut self, offset: A) -> SignalSourceBuilder<A> {
        self.offset = offset;
        self
    }

    pub fn with_amplitude(mut self, amplitude: A) -> SignalSourceBuilder<A> {
        self.amplitude = amplitude;
        self
    }

    pub fn with_initial_phase(mut self, initial_phase: f32) -> SignalSourceBuilder<A> {
        self.initial_phase = initial_phase;
        self
    }

    fn nco(&self) -> NCO {
        NCO::new(
            self.initial_phase,
            2.0 * core::f32::consts::PI * self.frequency / self.sampling_freq,
        )
    }

    pub fn build<F>(&self, f: F) -> Block
    where
        F: FnMut(FixedPointPhase) -> A + Send + 'static,
    {
        SignalSource::new(f, self.nco(), self.amplitude, self.offset)
    }
}

impl Default for SignalSourceBuilder<f32> {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for SignalSourceBuilder<Complex32> {
    fn default() -> Self {
        Self::new()
    }
}
