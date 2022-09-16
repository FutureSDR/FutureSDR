use async_io::Timer;
use std::cmp;
use std::ptr;
use std::time::Duration;
use std::time::Instant;

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

/// Limit sample rate.
///
/// # Inputs
///
/// `in`: Input
///
/// # Outputs
///
/// `out`: Output
///
/// # Usage
/// ```
/// use futuresdr::blocks::Throttle;
/// use futuresdr::runtime::Flowgraph;
/// use num_complex::Complex;
///
/// let mut fg = Flowgraph::new();
///
/// let throttle = fg.add_block(Throttle::<Complex<f32>>::new(1_000_000.0));
/// ```
#[cfg_attr(docsrs, doc(cfg(not(target_arch = "wasm32"))))]
pub struct Throttle<T: Send + 'static> {
    rate: f64,
    t_init: Instant,
    n_items: usize,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> Throttle<T> {
    /// Creates a new Throttle block which will throttle to the specified rate.
    pub fn new(rate: f64) -> Block {
        Block::new(
            BlockMetaBuilder::new("Throttle").build(),
            StreamIoBuilder::new()
                .add_input("in", std::mem::size_of::<T>())
                .add_output("out", std::mem::size_of::<T>())
                .build(),
            MessageIoBuilder::<Self>::new().build(),
            Throttle::<T> {
                rate,
                t_init: Instant::now(),
                n_items: 0,
                _type: std::marker::PhantomData,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: Send + 'static> Kernel for Throttle<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<u8>();
        let o = sio.output(0).slice::<u8>();
        let item_size = std::mem::size_of::<T>();

        let mut m = cmp::min(i.len(), o.len());

        let now = Instant::now();
        let target_items = (now - self.t_init).as_secs_f64() * self.rate;
        let target_items = target_items.floor() as usize;

        m = cmp::min(m, (target_items - self.n_items) * item_size) as usize;
        if m != 0 {
            unsafe {
                ptr::copy_nonoverlapping(i.as_ptr(), o.as_mut_ptr(), m);
            }

            let m = m / item_size;
            self.n_items += m;
            sio.input(0).consume(m);
            sio.output(0).produce(m);
        }

        if sio.input(0).finished() && i.len() == m * item_size {
            io.finished = true;
        }

        io.block_on(async {
            Timer::after(Duration::from_millis(100)).await;
        });

        Ok(())
    }

    async fn init(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        self.t_init = Instant::now();
        self.n_items = 0;
        Ok(())
    }
}
