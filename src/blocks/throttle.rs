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
use async_io::Timer;
use std::time::Duration;
use std::time::Instant;

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
pub struct Throttle<T: Copy + Send + 'static> {
    rate: f64,
    t_init: Instant,
    n_items: usize,
    _type: std::marker::PhantomData<T>,
}

impl<T: Copy + Send + 'static> Throttle<T> {
    /// Creates a new Throttle block which will throttle to the specified rate.
    pub fn new(rate: f64) -> Block {
        Block::new(
            BlockMetaBuilder::new("Throttle").build(),
            StreamIoBuilder::new()
                .add_input::<T>("in")
                .add_output::<T>("out")
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
impl<T: Copy + Send + 'static> Kernel for Throttle<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<T>();
        let o = sio.output(0).slice::<T>();

        let now = Instant::now();
        let target_items = (now - self.t_init).as_secs_f64() * self.rate;
        let target_items = target_items.floor() as usize;
        let remaining_items = target_items - self.n_items;

        let m = *[remaining_items, i.len(), o.len()]
            .iter()
            .min()
            .unwrap_or(&0);

        if m != 0 {
            o[..m].copy_from_slice(&i[..m]);
            self.n_items += m;
            sio.input(0).consume(m);
            sio.output(0).produce(m);
        }

        if sio.input(0).finished() && i.len() == m {
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
