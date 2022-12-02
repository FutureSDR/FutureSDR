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


pub struct AGC {
    // Minimum value that has to be reached in order for AGC to start adjusting gain.
    squelch: f32,
    // maximum gain value (0 for unlimited).
    max_gain: f32,
    // initial gain value.
    gain: f32,
    // reference value to adjust signal power to.
    reference_power: f32,
    // the update rate of the loop.
    adjustment_rate: f32,
    // (Boolean) Set when gain should not be adjusted anymore, but rather be locked to the current value
    gain_lock: u32,
}

impl AGC
{
    pub fn new(
        squelch: f32,
        max_gain: f32,
        gain: f32,
        adjustment_rate: f32,
        reference_power: f32,
        gain_lock: u32,
    ) -> Block {
        Block::new(
            BlockMetaBuilder::new("AGC").build(),
            StreamIoBuilder::new()
                .add_input::<f32>("in")
                .add_output::<f32>("out")
                .build(),
            MessageIoBuilder::<Self>::new()
                .add_input("gain_lock",
                           |block: &mut AGC,
                            _mio: &mut MessageIo<AGC>,
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
                           |block: &mut AGC,
                            _mio: &mut MessageIo<AGC>,
                            _meta: &mut BlockMeta,
                            p: Pmt| {
                               async move {
                                   if let Pmt::F32(ref r) = &p {
                                       block.max_gain = *r;
                                   }
                                   Ok(p)
                               }.boxed()
                           },
                )
                .add_input("adjustment_rate",
                           |block: &mut AGC,
                            _mio: &mut MessageIo<AGC>,
                            _meta: &mut BlockMeta,
                            p: Pmt| {
                               async move {
                                   if let Pmt::F32(ref r) = &p {
                                       block.adjustment_rate = *r;
                                   }
                                   Ok(p)
                               }.boxed()
                           },
                )
                .add_input("reference_power",
                           |block: &mut AGC,
                            _mio: &mut MessageIo<AGC>,
                            _meta: &mut BlockMeta,
                            p: Pmt| {
                               async move {
                                   if let Pmt::F32(ref r) = &p {
                                       block.reference_power = *r;
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
                adjustment_rate,
                gain_lock,
            },
        )
    }

    #[inline(always)]
    fn scale(&mut self, input: f32) -> f32 {
        let output = input * self.gain;
        if self.gain_lock == 0 {
            self.gain += (self.reference_power - output.abs()) * self.adjustment_rate;
            if self.max_gain.abs() > 0.0 && self.gain.abs() > self.max_gain.abs() {
                self.gain = self.max_gain;
            }
        }
        output
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for AGC
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<f32>();
        let o = sio.output(0).slice::<f32>();

        let m = std::cmp::min(i.len(), o.len());
        if m > 0 {
            for (src, dst) in i.iter().zip(o.iter_mut()) {
                if src.abs() > self.squelch.abs() {
                    *dst = self.scale(*src);
                } else {
                    *dst = 0.0;
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

pub struct AGCBuilder
{
    squelch: f32,
    // maximum gain value (0 for unlimited).
    max_gain: f32,
    // initial gain value.
    gain: f32,
    // reference value to adjust signal power to.
    reference_power: f32,
    // the update rate of the loop.
    adjustment_rate: f32,
    // (Boolean) Set when gain should not be adjusted anymore, but rather be locked to the current value
    gain_lock: u32,
}

impl AGCBuilder
{
    pub fn new() -> AGCBuilder {
        AGCBuilder {
            squelch: 0.0,
            max_gain: 65536.0,
            gain: 1.0,
            reference_power: 1.0,
            adjustment_rate: 0.0001,
            gain_lock: 0,
        }
    }

    pub fn squelch(mut self, squelch: f32) -> AGCBuilder {
        self.squelch = squelch;
        self
    }

    pub fn max_gain(mut self, max_gain: f32) -> AGCBuilder {
        self.max_gain = max_gain;
        self
    }

    pub fn adjustment_rate(mut self, adjustment_rate: f32) -> AGCBuilder {
        self.squelch = adjustment_rate;
        self
    }

    pub fn reference_power(mut self, reference_power: f32) -> AGCBuilder {
        self.reference_power = reference_power;
        self
    }

    pub fn gain_lock(mut self, gain_lock: bool) -> AGCBuilder {
        self.gain_lock = gain_lock as u32;
        self
    }

    pub fn build(self) -> Block {
        AGC::new(
            self.squelch,
            self.max_gain,
            self.gain,
            self.adjustment_rate,
            self.reference_power,
            self.gain_lock,
        )
    }
}