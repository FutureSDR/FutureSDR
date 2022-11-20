use num_complex::Complex32;
use num_complex::ComplexFloat;
use soapysdr::Device;
use soapysdr::Range;

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


pub struct AGC<T>
{
    _type: std::marker::PhantomData<T>,
    squelch: f32,
    target: f32,
    chunk_size: usize,
    dev: Option<Device>,
    gain: f64,
    gain_range: Range,
}

impl AGC<f32>
{
    pub fn new(
        squelch: f32,
        target: f32,
        dev: Option<Device>,
    ) -> Block {
        let mut gain = 0.0;
        let mut gain_range = soapysdr::Range{ minimum: 0.0, maximum: 1.0, step: 1.0 };
        if let Some(ref device) = dev {
            gain = device.gain(soapysdr::Direction::Rx, 0).unwrap();
            gain_range = device.gain_range(soapysdr::Direction::Rx, 0).unwrap();
        }
        Block::new(
            BlockMetaBuilder::new("AGC").build(),
            StreamIoBuilder::new()
                .add_input::<f32>("in")
                .add_output::<f32>("out")
                .build(),
            MessageIoBuilder::<Self>::new().build(),
            AGC {
                _type: std::marker::PhantomData,
                squelch,
                target,
                chunk_size: 16,
                dev,
                gain,
                gain_range,
            },
        )
    }
}

impl AGC<Complex32> {
    pub fn new(
        squelch: f32,
        target: f32,
        dev: Option<Device>,
    ) -> Block {
        let mut gain = 0.0;
        let mut gain_range = soapysdr::Range{ minimum: 0.0, maximum: 1.0, step: 1.0 };
        if let Some(ref device) = dev {
            gain = device.gain(soapysdr::Direction::Rx, 0).unwrap();
            gain_range = device.gain_range(soapysdr::Direction::Rx, 0).unwrap();
        }
        Block::new(
            BlockMetaBuilder::new("AGC").build(),
            StreamIoBuilder::new()
                .add_input::<f32>("in")
                .add_output::<f32>("out")
                .build(),
            MessageIoBuilder::<Self>::new().build(),
            AGC {
                _type: std::marker::PhantomData,
                squelch,
                target,
                chunk_size: 16,
                dev,
                gain,
                gain_range,
            },
        )
    }
}


#[doc(hidden)]
#[async_trait]
impl Kernel for AGC<f32> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<f32>();
        let o = sio.output(0).slice::<f32>();

        /*info!("YEA: ");
        if let Some(ref device) = self.dev {
            let has_gain_mode = device.has_gain_mode(soapysdr::Direction::Rx, 0).unwrap();
            info!("Has Gain Mode: {}", has_gain_mode);

            let agc_enabled = device.gain_mode(soapysdr::Direction::Rx, 0).unwrap();
            info!("AGC Enabled: {}", agc_enabled);

            let gain_range = device.gain_range(soapysdr::Direction::Rx, 0).unwrap();
            info!("Gain Range: {:?}", gain_range);

            let gain = device.gain(soapysdr::Direction::Rx, 0).unwrap();
            info!("Gain: {}", gain);
        }*/

        // Max I/Q absolute value over the whole buffer
        let mut max: f32 = 0.;

        let m = std::cmp::min(i.len(), o.len());
        if m > 0 {
            for (i_chunk, o_chunk) in i.chunks(self.chunk_size).zip(o.chunks_mut(self.chunk_size)) {
                let factor: f32 = i_chunk.iter().map(|v| v.abs()).reduce(f32::max).unwrap(); // Maximum
                //let factor = i_chunk.iter().map(|v| v.abs()).sum::<f32>() / (i_chunk.len() as f32); // Average
                let scale = self.target / factor;

                max = max.max(factor);

                for (src, dst) in i_chunk.iter().zip(o_chunk.iter_mut()) {
                    if src.abs().gt(&self.squelch) {
                        *dst = (*src) * scale;
                    } else {
                        *dst = 0.;
                    }
                }
            }

            sio.input(0).consume(m);
            sio.output(0).produce(m);
        }

        if sio.input(0).finished() && m == i.len() {
            io.finished = true;
        }

        // Adjust gain on Hardware
        if let Some(ref device) = self.dev {
            info!("Max: {}, Gain: {}, Gain Range: {:?}", max, self.gain, self.gain_range);

            /*if max < 1.0 {
                let mut gain = device.gain(soapysdr::Direction::Rx, 0).unwrap();
                info!("Gain: {}", gain);
                let gain_range: soapysdr::Range = device.gain_range(soapysdr::Direction::Rx, 0).unwrap();
                info!("Gain Range: {:?}", gain_range);
                if gain < 60. {
                    gain += 1.0;
                    device.set_gain(soapysdr::Direction::Rx, 0, gain).unwrap();
                }
            }*/
        }

        Ok(())
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for AGC<Complex32> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<Complex32>();
        let o = sio.output(0).slice::<Complex32>();

        let m = std::cmp::min(i.len(), o.len());
        if m > 0 {
            for (i_chunk, o_chunk) in i.chunks(self.chunk_size).zip(o.chunks_mut(self.chunk_size)) {
                let (factor_re, factor_im) = i_chunk.iter().map(|v| (v.re.abs(), v.im.abs())).reduce(|accum, (re, im)| {
                    (if accum.0 >= re { accum.0 } else { re },
                     if accum.1 >= im { accum.1 } else { im })
                }).unwrap(); // Maximum
                let (scale_re, scale_im) = ((self.target / factor_re), (self.target / factor_im));

                for (src, dst) in i_chunk.iter().zip(o_chunk.iter_mut()) {
                    if src.abs().gt(&self.squelch) {
                        *dst = (*src) * Complex32::new(scale_re, scale_im);
                    } else {
                        *dst = Complex32::new(0.0, 0.0);
                    }
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

/*pub struct AGCBuilder<T> {
    _type: std::marker::PhantomData<T>,
    squelch: f32,
    target: f32,
}

impl<T> AGCBuilder<T> {
    pub fn new() -> AGCBuilder<T> {
        AGCBuilder {
            _type: std::marker::PhantomData,
            squelch: 0.0,
            target: 1.0,
        }
    }

    pub fn squelch(mut self, squelch: f32) -> AGCBuilder<T> {
        self.squelch = squelch;
        self
    }

    pub fn target(mut self, target: f32) -> AGCBuilder<T> {
        self.target = target;
        self
    }

    pub fn build(self) -> Block {
        AGC::<T>::new(self.squelch, self.target)
    }
}

impl<T> Default for AGCBuilder<T> {
    fn default() -> Self {
        Self::new()
    }
}*/