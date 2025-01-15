use lttng_ust::import_tracepoints;

use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageOutputs;
use crate::runtime::MessageOutputsBuilder;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

import_tracepoints!(concat!(env!("OUT_DIR"), "/tracepoints.rs"), tracepoints);

/// Null sink that calls an [lttng](https://lttng.org/) tracepoint for every batch of received samples.
#[derive(Block)]
pub struct NullSink<T: Send + 'static> {
    n_received: u64,
    probe_granularity: u64,
    id: Option<u64>,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> NullSink<T> {
    /// Create NullSink block
    pub fn new(probe_granularity: u64) -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("LTTngNullSink").build(),
            StreamIoBuilder::new().add_input::<T>("in").build(),
            MessageOutputsBuilder::new().build(),
            NullSink::<T> {
                n_received: 0,
                probe_granularity,
                id: None,
                _type: std::marker::PhantomData,
            },
        )
    }
    /// Get number of received samples
    pub fn n_received(&self) -> u64 {
        self.n_received
    }
}

#[doc(hidden)]
impl<T: Send + 'static> Kernel for NullSink<T> {
    async fn init(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageOutputs,
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
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice_unchecked::<u8>();
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
