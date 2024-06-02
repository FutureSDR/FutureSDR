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

#[cfg(feature = "telemetry")]
use crate::telemetry::opentelemetry::KeyValue;

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

    #[cfg(feature = "telemetry")]
    telemetry_resource: crate::telemetry::TelemetryResource,
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
                #[cfg(feature = "telemetry")]
                telemetry_resource: {
                    crate::telemetry::TelemetryResource::new(
                        "MessageSourceTelemetry".to_string(),
                        env!("CARGO_PKG_VERSION").to_lowercase(),
                    )
                },
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
        #[cfg(feature = "telemetry")]
        let agc_gauge = {
            /* println!(
                "Collecting the following metrics: {:?}",
                _meta.telemetry_config().active_metrics()
            ); */

            let meter = self.telemetry_resource.get_meter();
            let gauge = meter
                .f64_gauge("agc_gauge")
                .with_description("Gauge to measure AGC parameters")
                .with_unit("dB")
                .init();

            gauge
        };

        let i = sio.input(0).slice::<T>();
        let o = sio.output(0).slice::<T>();

        let m = std::cmp::min(i.len(), o.len());
        if m > 0 {
            for (src, dst) in i.iter().zip(o.iter_mut()) {
                let input_power = src.abs().to_f32().unwrap();
                if input_power > self.squelch {
                    *dst = self.scale(*src);
                } else {
                    *dst = T::from(0.0).unwrap();
                }
                let output_power = (*dst).abs().to_f32().unwrap();

                #[cfg(feature = "telemetry")]
                if _meta
                    .telemetry_config()
                    .active_metrics()
                    .contains("agc_stats")
                {
                    // println!("Collecting AGC telemetry data");
                    agc_gauge.record(input_power.into(), &[KeyValue::new("type", "input_power")]);
                    agc_gauge.record(
                        output_power.into(),
                        &[KeyValue::new("type", "output_power")],
                    );
                    // We need a force_flush() here on the meter_provider to record the exact values and dont aggregate them over time.
                    // Might have to wait for implementation here: https://github.com/open-telemetry/opentelemetry-specification/issues/617
                }
            }

            #[cfg(feature = "telemetry")]
            if _meta
                .telemetry_config()
                .active_metrics()
                .contains("agc_stats")
            {
                agc_gauge.record(self.squelch.into(), &[KeyValue::new("type", "squelch")]);
                agc_gauge.record(
                    self.reference_power.into(),
                    &[KeyValue::new("type", "reference_power")],
                );
                // We need a force_flush() here on the meter_provider to record the exact values and dont aggregate them over time.
                // Might have to wait for implementation here: https://github.com/open-telemetry/opentelemetry-specification/issues/617
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
