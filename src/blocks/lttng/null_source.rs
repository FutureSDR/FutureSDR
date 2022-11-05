use lttng_ust::import_tracepoints;
use std::ptr;

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

/// Null source that calls an [lttng](https://lttng.org/) tracepoint for every batch of produced samples.
pub struct NullSource<T: Send + 'static> {
    probe_granularity: u64,
    id: Option<u64>,
    n_produced: u64,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> NullSource<T> {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(probe_granularity: u64) -> Block {
        Block::new(
            BlockMetaBuilder::new("LTTngNullSource").build(),
            StreamIoBuilder::new().add_output::<T>("out").build(),
            MessageIoBuilder::new().build(),
            NullSource::<T> {
                probe_granularity,
                id: None,
                n_produced: 0,
                _type: std::marker::PhantomData,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: Send + 'static> Kernel for NullSource<T> {
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
        _io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let o = sio.output(0).slice_unchecked::<u8>();

        unsafe {
            ptr::write_bytes(o.as_mut_ptr(), 0, o.len());
        }

        let before = self.n_produced / self.probe_granularity;
        let n = o.len() / std::mem::size_of::<T>();
        sio.output(0).produce(n);
        self.n_produced += n as u64;
        let after = self.n_produced / self.probe_granularity;

        for i in 1..=(after - before) {
            tracepoints::futuresdr::tx(self.id.unwrap(), before + i);
        }

        Ok(())
    }
}
