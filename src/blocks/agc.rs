use num_complex::ComplexFloat;
use rustfft::num_traits::ToPrimitive;

use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Pmt;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

/// Automatic Gain Control Block
pub struct Agc<T> {
    /// Minimum value that has to be reached in order for AGC to start adjusting gain.
    squelch: f32,
    /// maximum gain value
    max_gain: f32,
    /// initial gain value.
    gain: f32,
    /// reference value to adjust signal power to.
    reference_power: f32,
    /// the update rate of the loop.
    adjustment_rate: f32,
    /// Set when gain should not be adjusted anymore, but rather be locked to the current value
    gain_locked: bool,
    _type: std::marker::PhantomData<T>,
}

impl<T> Agc<T>
where
    T: Send + Sync + ComplexFloat + 'static,
{
    /// Create AGC Block
    ///
    /// ## Parameter
    /// - `squelch`: surpress anything below this level
    /// - `max_gain`: maximum gain setting
    /// - `gain`: initial gain setting
    /// - `reference_power`: target power level
    /// - `gain_locked`: lock gain to fixed value
    ///
    /// ## Message Handler
    ///
    /// - `gain_locked`: set `gain_locked` parameter with a [`Pmt::Bool`].
    /// - `max_gain`: set `max_gain` parameter with a [`Pmt::F32`].
    /// - `adjustment_rate`: set `adjustment_rate` with a [`Pmt::F32`].
    /// - `reference_power`: set `reference_power` with a [`Pmt::F32`].
    ///
    /// ## Stream Input
    /// - `in`: Input stream of items, implementing [`ComplexFloat`]
    ///
    /// ## Stream Output
    /// - `out`: Leveled output items of same type as `in` stream.
    pub fn new(
        squelch: f32,
        max_gain: f32,
        gain: f32,
        adjustment_rate: f32,
        reference_power: f32,
        gain_locked: bool,
    ) -> Block {
        assert!(max_gain >= 0.0);
        assert!(squelch >= 0.0);

        Block::new(
            BlockMetaBuilder::new("AGC").build(),
            StreamIoBuilder::new()
                .add_input::<T>("in")
                .add_output::<T>("out")
                .build(),
            MessageIoBuilder::<Self>::new()
                .add_input("gain_locked", Self::gain_locked)
                .add_input("max_gain", Self::max_gain)
                .add_input("adjustment_rate", Self::adjustment_rate)
                .add_input("reference_power", Self::reference_power)
                .build(),
            Agc {
                squelch,
                max_gain,
                gain,
                reference_power,
                adjustment_rate,
                gain_locked,
                _type: std::marker::PhantomData,
            },
        )
    }

    #[message_handler]
    async fn gain_locked(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        if let Pmt::Bool(l) = p {
            self.gain_locked = l;
            Ok(Pmt::Ok)
        } else {
            Ok(Pmt::InvalidValue)
        }
    }

    #[message_handler]
    async fn max_gain(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        if let Pmt::F32(r) = p {
            self.max_gain = r;
            Ok(Pmt::Ok)
        } else {
            Ok(Pmt::InvalidValue)
        }
    }

    #[message_handler]
    async fn adjustment_rate(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        if let Pmt::F32(r) = p {
            self.adjustment_rate = r;
            Ok(Pmt::Ok)
        } else {
            Ok(Pmt::InvalidValue)
        }
    }

    #[message_handler]
    async fn reference_power(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        if let Pmt::F32(r) = p {
            self.reference_power = r;
            Ok(Pmt::Ok)
        } else {
            Ok(Pmt::InvalidValue)
        }
    }

    #[inline(always)]
    fn scale(&mut self, input: T) -> T {
        let output = input * T::from(self.gain).unwrap();
        if !self.gain_locked {
            self.gain +=
                (self.reference_power - output.abs().to_f32().unwrap()) * self.adjustment_rate;
            self.gain = self.gain.min(self.max_gain);
        }
        output
    }
}

#[doc(hidden)]
#[async_trait]
impl<T> Kernel for Agc<T>
where
    T: Send + Sync + ComplexFloat + 'static,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<T>();
        let o = sio.output(0).slice::<T>();

        let m = std::cmp::min(i.len(), o.len());
        if m > 0 {
            for (src, dst) in i.iter().zip(o.iter_mut()) {
                if src.abs().to_f32().unwrap() > self.squelch {
                    *dst = self.scale(*src);
                } else {
                    *dst = T::from(0.0).unwrap();
                }
            }

            sio.input(0).consume(m);
            sio.output(0).produce(m);
        }

        if sio.input(0).finished() && m == i.len() {
            io.finished = true;
        }

        Ok(())
    }
}

/// Builder for [`Agc`] block
pub struct AgcBuilder<T>
where
    T: Send + Sync + ComplexFloat + 'static,
{
    squelch: f32,
    /// maximum gain value (0 for unlimited).
    max_gain: f32,
    /// initial gain value.
    gain: f32,
    /// reference value to adjust signal power to.
    reference_power: f32,
    /// the update rate of the loop.
    adjustment_rate: f32,
    /// Set when gain should not be adjusted anymore, but rather be locked to the current value
    gain_locked: bool,
    _type: std::marker::PhantomData<T>,
}

impl<T> AgcBuilder<T>
where
    T: Send + Sync + ComplexFloat + 'static,
{
    /// Create builder w/ default parameters
    ///
    /// ## Defaults
    /// - `squelch`: 0.0
    /// - `max_gain`: 65536.0
    /// - `gain`: 1.0
    /// - `reference_power`: 1.0
    /// - `adjustment_rate`: 0.0001
    /// - `gain_locked`: false
    pub fn new() -> AgcBuilder<T> {
        AgcBuilder {
            squelch: 0.0,
            max_gain: 65536.0,
            gain: 1.0,
            reference_power: 1.0,
            adjustment_rate: 0.0001,
            gain_locked: false,
            _type: std::marker::PhantomData,
        }
    }

    /// Surpress signals below this level
    pub fn squelch(mut self, squelch: f32) -> AgcBuilder<T> {
        self.squelch = squelch;
        self
    }

    /// Max gain to use to bring input closer to reference level
    pub fn max_gain(mut self, max_gain: f32) -> AgcBuilder<T> {
        self.max_gain = max_gain;
        self
    }

    /// Adjustment rate, i.e., impact of current sample on gain setting
    pub fn adjustment_rate(mut self, adjustment_rate: f32) -> AgcBuilder<T> {
        self.adjustment_rate = adjustment_rate;
        self
    }

    /// Targeted power level
    pub fn reference_power(mut self, reference_power: f32) -> AgcBuilder<T> {
        self.reference_power = reference_power;
        self
    }

    /// Fix gain setting, disabling AGC
    pub fn gain_locked(mut self, gain_locked: bool) -> AgcBuilder<T> {
        self.gain_locked = gain_locked;
        self
    }

    /// Create [`Agc`] block
    pub fn build(self) -> Block {
        Agc::<T>::new(
            self.squelch,
            self.max_gain,
            self.gain,
            self.adjustment_rate,
            self.reference_power,
            self.gain_locked,
        )
    }
}

impl<T: ComplexFloat + Send + Sync> Default for AgcBuilder<T> {
    fn default() -> Self {
        Self::new()
    }
}
