use anyhow::Result;
use async_io::Timer;
use std::cmp;
use std::ptr;
use std::time::Duration;
use std::time::Instant;

use crate::runtime::AsyncKernel;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

pub struct Throttle {
    item_size: usize,
    rate: f64,
    t_init: Instant,
    n_items: usize,
}

impl Throttle {
    pub fn new(item_size: usize, rate: f64) -> Block {
        Block::new_async(
            BlockMetaBuilder::new("Throttle").build(),
            StreamIoBuilder::new()
                .add_input("in", item_size)
                .add_output("out", item_size)
                .build(),
            MessageIoBuilder::<Throttle>::new().build(),
            Throttle {
                item_size,
                rate,
                t_init: Instant::now(),
                n_items: 0,
            },
        )
    }
}

#[async_trait]
impl AsyncKernel for Throttle {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<u8>();
        let o = sio.output(0).slice::<u8>();

        debug_assert_eq!(i.len() % self.item_size, 0);
        debug_assert_eq!(o.len() % self.item_size, 0);

        let mut m = cmp::min(i.len(), o.len());

        let now = Instant::now();
        let target_items = (now - self.t_init).as_secs_f64() * self.rate;
        let target_items = target_items.floor() as usize;

        m = cmp::min(m, (target_items - self.n_items) * self.item_size) as usize;
        if m != 0 {
            unsafe {
                ptr::copy_nonoverlapping(i.as_ptr(), o.as_mut_ptr(), m);
            }

            let m = m / self.item_size;
            self.n_items += m;
            sio.input(0).consume(m);
            sio.output(0).produce(m);
        }

        if sio.input(0).finished() && i.len() == m * self.item_size {
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

pub struct ThrottleBuilder {
    item_size: usize,
    rate: f64,
}

impl ThrottleBuilder {
    pub fn new(item_size: usize, rate: f64) -> ThrottleBuilder {
        ThrottleBuilder { item_size, rate }
    }

    pub fn build(self) -> Block {
        Throttle::new(self.item_size, self.rate)
    }
}
