use num_complex::ComplexFloat;
use futuresdr_pmt::Pmt;
use futures::FutureExt;

use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;


pub struct AGC<T> {
    // Minimum value that has to be reached in order for AGC to start adjusting gain.
    squelch: T,
    // maximum gain value (0 for unlimited).
    max_gain: T,
    // initial gain value.
    gain: T,
    // reference value to adjust signal power to.
    reference_power: T,
    // the update rate of the loop.
    update_rate: T,
    // (Boolean) Set when gain should not be adjusted anymore, but rather be locked to the current value
    gain_lock: u32,
}

impl<T> AGC<T>
    where
        T: Send + Sync + ComplexFloat + 'static,
{
    pub fn new(
        squelch: T,
        max_gain: T,
        gain: T,
        update_rate: T,
        reference_power: T,
        gain_lock: u32,
    ) -> Block {
        Block::new(
            BlockMetaBuilder::new("AGC").build(),
            StreamIoBuilder::new()
                .add_input::<T>("in")
                .add_output::<T>("out")
                .build(),
            MessageIoBuilder::<Self>::new()
                .add_input("gain_lock",
                           |block: &mut AGC<T>,
                            _mio: &mut MessageIo<AGC<T>>,
                            _meta: &mut BlockMeta,
                            p: Pmt| {
                               async move {
                                   if let Pmt::U32(ref r) = &p {
                                       block.gain_lock = *r;
                                   }
                                   Ok(p)
                               }.boxed()
                           },
                )
                .add_input("max_gain",
                           |block: &mut AGC<T>,
                            _mio: &mut MessageIo<AGC<T>>,
                            _meta: &mut BlockMeta,
                            p: Pmt| {
                               async move {
                                   if let Pmt::Any(ref r) = &p {
                                       match r.downcast_ref::<T>() {
                                           Some(v) => {
                                               block.max_gain = *v;
                                               println!("max_gain type: {:?}", r.type_id());
                                           }
                                           None => {
                                               println!("unknown type: {:?}", r.type_id());
                                           }
                                       }
                                   }
                                   Ok(p)
                               }.boxed()
                           },
                )
                .add_input("update_rate",
                           |block: &mut AGC<T>,
                            _mio: &mut MessageIo<AGC<T>>,
                            _meta: &mut BlockMeta,
                            p: Pmt| {
                               async move {
                                   if let Pmt::Any(ref r) = &p {
                                       match r.downcast_ref::<T>() {
                                           Some(v) => {
                                               block.update_rate = *v;
                                               println!("update_rate type: {:?}", r.type_id());
                                           }
                                           None => {
                                               println!("unknown type: {:?}", r.type_id());
                                           }
                                       }
                                   }
                                   Ok(p)
                               }.boxed()
                           },
                )
                .add_input("reference_power",
                           |block: &mut AGC<T>,
                            _mio: &mut MessageIo<AGC<T>>,
                            _meta: &mut BlockMeta,
                            p: Pmt| {
                               async move {
                                   if let Pmt::Any(ref r) = &p {
                                       match r.downcast_ref::<T>() {
                                           Some(v) => {
                                               block.reference_power = *v;
                                               println!("reference_power type: {:?}", r.type_id());
                                           }
                                           None => {
                                               println!("unknown type: {:?}", r.type_id());
                                           }
                                       }
                                   }
                                   Ok(p)
                               }.boxed()
                           },
                ).build(),
            AGC {
                squelch,
                max_gain,
                gain,
                reference_power,
                update_rate,
                gain_lock,
            },
        )
    }

    #[inline(always)]
    fn scale(&mut self, input: T) -> T {
        let output = input * self.gain;
        if self.gain_lock == 0 {
            // output.abs() might perform better than output.powi(2).sqrt()
            self.gain = self.gain + (self.reference_power - output.powi(2).sqrt()) * self.update_rate;
            if self.max_gain.abs() > T::from(0.0).unwrap().abs() && self.gain.abs() > self.max_gain.abs() {
                self.gain = self.max_gain;
            }
        }
        output
    }
}

#[doc(hidden)]
#[async_trait]
impl<T> Kernel for AGC<T>
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
                if src.abs() > self.squelch.abs() {
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

pub struct AGCBuilder<T>
    where
        T: Send + Sync + ComplexFloat + 'static
{
    squelch: T,
    // maximum gain value (0 for unlimited).
    max_gain: T,
    // initial gain value.
    gain: T,
    // reference value to adjust signal power to.
    reference_power: T,
    // the update rate of the loop.
    update_rate: T,
    // (Boolean) Set when gain should not be adjusted anymore, but rather be locked to the current value
    gain_lock: u32,
}

impl<T> AGCBuilder<T>
    where
        T: Send + Sync + ComplexFloat + 'static
{
    pub fn new() -> AGCBuilder<T> {
        AGCBuilder {
            squelch: T::from(0.0).unwrap(),
            max_gain: T::from(65536.0).unwrap(),
            gain: T::from(1.0).unwrap(),
            reference_power: T::from(1.0).unwrap(),
            update_rate: T::from(0.0001).unwrap(),
            gain_lock: 0,
        }
    }

    pub fn squelch(mut self, squelch: T) -> AGCBuilder<T> {
        self.squelch = squelch;
        self
    }

    pub fn max_gain(mut self, max_gain: T) -> AGCBuilder<T> {
        self.max_gain = max_gain;
        self
    }

    pub fn update_rate(mut self, update_rate: T) -> AGCBuilder<T> {
        self.squelch = update_rate;
        self
    }

    pub fn reference_power(mut self, reference_power: T) -> AGCBuilder<T> {
        self.reference_power = reference_power;
        self
    }

    pub fn gain_lock(mut self, gain_lock: bool) -> AGCBuilder<T> {
        self.gain_lock = gain_lock as u32;
        self
    }

    pub fn build(self) -> Block {
        AGC::<T>::new(
            self.squelch,
            self.max_gain,
            self.gain,
            self.update_rate,
            self.reference_power,
            self.gain_lock,
        )
    }
}