use num_complex::Complex32;
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


pub struct AGC<T>
{
    _type: std::marker::PhantomData<T>,
    squelch: f32,
    target: f32,
    chunk_size: usize,
    sw_gain_lock: u32,
    sw_scale: f32,
}

impl AGC<f32> {
    pub fn new(
        squelch: f32,
        target: f32,
    ) -> Block {
        Block::new(
            BlockMetaBuilder::new("AGC").build(),
            StreamIoBuilder::new()
                .add_input::<f32>("in")
                .add_output::<f32>("out")
                .build(),
            MessageIoBuilder::<Self>::new()
                .add_input("lock_sw_gain",
                           |block: &mut AGC<f32>,
                            _mio: &mut MessageIo<AGC<f32>>,
                            _meta: &mut BlockMeta,
                            p: Pmt| {
                               async move {
                                   if let Pmt::U32(ref r) = &p {
                                       block.sw_gain_lock = *r;
                                       info!("sw_gain_lock: {}", block.sw_gain_lock);
                                   }
                                   Ok(p)
                               }.boxed()
                           },
                )
                .add_input("set_sw_scale",
                           |block: &mut AGC<f32>,
                            _mio: &mut MessageIo<AGC<f32>>,
                            _meta: &mut BlockMeta,
                            p: Pmt| {
                               async move {
                                   if let Pmt::F32(ref r) = &p {
                                       block.sw_scale = *r;
                                       info!("sw_scale: {}", block.sw_scale);
                                   }
                                   Ok(p)
                               }.boxed()
                           },
                ).build(),
            AGC {
                _type: std::marker::PhantomData,
                squelch,
                target,
                chunk_size: 16,
                sw_gain_lock: 0,
                sw_scale: 1.0,
            },
        )
    }
}

impl AGC<Complex32> {
    pub fn new(
        squelch: f32,
        target: f32,
    ) -> Block {
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
                sw_gain_lock: 0,
                sw_scale: 1.0,
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

        let m = std::cmp::min(i.len(), o.len());
        if m > 0 {
            for (i_chunk, o_chunk) in i.chunks(self.chunk_size).zip(o.chunks_mut(self.chunk_size)) {
                if self.sw_gain_lock != 0 {
                    let factor: f32 = i_chunk.iter().map(|v| v.abs()).reduce(f32::max).unwrap(); // Maximum
                    //let factor = i_chunk.iter().map(|v| v.abs()).sum::<f32>() / (i_chunk.len() as f32); // Average
                    self.sw_scale = self.target / factor;
                }

                for (src, dst) in i_chunk.iter().zip(o_chunk.iter_mut()) {
                    if src.abs().gt(&self.squelch) {
                        *dst = (*src) * self.sw_scale;
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