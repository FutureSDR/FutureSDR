use lttng_ust::import_tracepoints;

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

import_tracepoints!(concat!(env!("OUT_DIR"), "/tracepoints.rs"), tracepoints);

/// Null sink that calls an [lttng](https://lttng.org/) tracepoint for every batch of received samples.
pub struct NullSink<T: Send + 'static> {
    n_received: u64,
    probe_granularity: u64,
    id: Option<u64>,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> NullSink<T> {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(probe_granularity: u64) -> Block {
        Block::new(
            BlockMetaBuilder::new("LTTngNullSink").build(),
            StreamIoBuilder::new().add_input::<T>("in").build(),
            MessageIoBuilder::new().build(),
            NullSink::<T> {
                n_received: 0,
                probe_granularity,
                id: None,
                _type: std::marker::PhantomData,
            },
        )
    }

    pub fn n_received(&self) -> u64 {
        self.n_received
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: Send + 'static> Kernel for NullSink<T> {
    async fn init(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        meta: &mut BlockMeta,
    ) -> Result<()> {
        let s = meta.instance_name().unwrap();
        self.id = Some(s.split('_').next_back().unwrap().parse::<u64>().unwrap());
        Ok(())
    }

    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<u8>();
        let item_size = std::mem::size_of::<T>();

        let before = self.n_received / self.probe_granularity;

        let n = i.len() / item_size;
        if n > 0 {
            self.n_received += n as u64;
            sio.input(0).consume(n);
        }

        if sio.input(0).finished() {
            io.finished = true;
        }

        let after = self.n_received / self.probe_granularity;
        if before ^ after != 0 {
            tracepoints::futuresdr::rx(self.id.unwrap(), after);
        }
        Ok(())
    }
}
