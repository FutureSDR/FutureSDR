use futuresdr::macros::async_trait;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::Result;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::TypedBlock;
use futuresdr::runtime::WorkIo;
use lttng_ust::import_tracepoints;
use std::ptr;

import_tracepoints!(concat!(env!("OUT_DIR"), "/tracepoints.rs"), tracepoints);

/// Null source that calls an [lttng](https://lttng.org/) tracepoint for every batch of produced samples.
pub struct LttngSource<T: Send + 'static> {
    probe_granularity: u64,
    id: Option<u64>,
    n_produced: u64,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> LttngSource<T> {
    /// Create LttngSource block
    pub fn new(probe_granularity: u64) -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("LTTngLttngSource").build(),
            StreamIoBuilder::new().add_output::<T>("out").build(),
            MessageIoBuilder::new().build(),
            LttngSource::<T> {
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
impl<T: Send + 'static> Kernel for LttngSource<T> {
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
