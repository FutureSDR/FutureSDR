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
    gain_lock: bool,
    /// Set when gain should be automatically locked, when reference power is reached.
    auto_lock: bool,
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
    /// - `gain_lock`: lock gain to fixed value
    /// - `auto_lock`: lock gain, when reference power is reached
    ///
    /// ## Message Handler
    ///
    /// - `auto_lock`: set `auto_lock` parameter with a [`Pmt::Bool`].
    /// - `gain_lock`: set `gain_lock` parameter with a [`Pmt::Bool`].
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
        gain_lock: bool,
        auto_lock: bool,
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
                .add_input("auto_lock", Self::auto_lock)
                .add_input("gain_lock", Self::gain_lock)
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
                gain_lock,
                auto_lock,
                _type: std::marker::PhantomData,
            },
        )
    }

    #[message_handler]
    async fn auto_lock(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        if let Pmt::Bool(l) = p {
            self.auto_lock = l;
            Ok(Pmt::Ok)
        } else {
            Ok(Pmt::InvalidValue)
        }
    }

    #[message_handler]
    async fn gain_lock(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        if let Pmt::Bool(l) = p {
            self.gain_lock = l;
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
        // if the input power is very low or very high compared to the reference power, we still want to reach the reference power quickly.
        // Thus we should make the gain also dependent on the input_power in relation to the reference power.

        // Gain is only exactly 1.0 when the AGC block is freshly initialized
        // We then can set the gain to a suitable value to scale
        // the input power to e.g. 99% of the reference power immediately.
        if self.gain == 1.0 {
            self.gain = (self.reference_power / input.abs().to_f32().unwrap()) * 0.99;
        }

        // The gain adjustment rate should not be fixed, but depending
        // on the input power in relation to the reference power (Thus actually the gain).
        // We dont want the gain to be adjusted very strongly on rather weak signals,
        // as it might make them being flattened out after a short time.
        // Thus, if we have a high gain factor ( a multitude of 1.0 or very close to zero) -> log10(gain).abs() is:
        //   - getting closer to zero, we want bigger changes -> higher adjustment_rate value
        //   - getting bigger, we want smaller changes -> smaller adjustment_rate value
        let dynamic_adjustment_rate = self.adjustment_rate.powf(self.gain.log10().abs());

        let output_abs = output.abs().to_f32().unwrap();

        if self.auto_lock && !self.gain_lock {
            let input_abs = input.abs().to_f32().unwrap();
            // Two scenarios exist here:
            if input_abs > self.reference_power {
                // 1. Input power is greater than reference power (We are scaling the signal down)
                //    - As soon as the output power is smaller than the reference power, we lock the gain.
                if output_abs < self.reference_power {
                    self.gain_lock = true;
                    debug!("Locked gain at at {}", self.gain)
                }
            } else {
                // 2. Input power is smaller than reference power (We are scaling the signal up)
                //    - As soon as the output power is greater than the reference power, we lock the gain.
                if output_abs > self.reference_power {
                    self.gain_lock = true;
                    debug!("Locked gain at at {}", self.gain)
                }
            }
        }

        if !self.gain_lock {
            // Slow down AGC adjustments 1/adjustment_rate times
            self.gain *=
                1.0 + (self.reference_power / output_abs).log10() * dynamic_adjustment_rate;
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
                let input_power = src.abs().to_f32().unwrap();
                if input_power > self.squelch {
                    *dst = self.scale(*src);
                } else {
                    *dst = T::from(self.reference_power).unwrap();
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
    gain_lock: bool,
    /// Set when gain should be automatically locked, when reference power is reached.
    auto_lock: bool,
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
    /// - `gain_lock`: false
    /// - `auto_lock`: false
    pub fn new() -> AgcBuilder<T> {
        AgcBuilder {
            squelch: 0.0,
            max_gain: 65536.0,
            gain: 1.0,
            reference_power: 1.0,
            adjustment_rate: 0.0001,
            gain_lock: false,
            auto_lock: false,
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
    pub fn gain_lock(mut self, gain_lock: bool) -> AgcBuilder<T> {
        self.gain_lock = gain_lock;
        self
    }

    /// Activate gain auto_locking, when the target reference power is reached
    pub fn auto_lock(mut self, auto_lock: bool) -> AgcBuilder<T> {
        self.auto_lock = auto_lock;
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
            self.gain_lock,
            self.auto_lock,
        )
    }
}

impl<T: ComplexFloat + Send + Sync> Default for AgcBuilder<T> {
    fn default() -> Self {
        Self::new()
    }
}
