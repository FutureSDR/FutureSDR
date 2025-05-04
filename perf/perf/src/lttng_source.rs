use futuresdr::prelude::*;
use lttng_ust::import_tracepoints;

import_tracepoints!(concat!(env!("OUT_DIR"), "/tracepoints.rs"), tracepoints);

/// Null source that calls an [lttng](https://lttng.org/) tracepoint for every batch of produced samples.
#[derive(Block)]
pub struct LttngSource<T, O = circular::Writer<T>>
where
    T: Send + Clone + 'static,
    O: CpuBufferWriter<Item = T>,
{
    #[output]
    output: O,
    probe_granularity: u64,
    id: Option<u64>,
    n_produced: u64,
}

impl<T, O> LttngSource<T, O>
where
    T: Send + Clone + 'static,
    O: CpuBufferWriter<Item = T>,
{
    /// Create LttngSource block
    pub fn new(probe_granularity: u64) -> Self {
        Self {
            output: O::default(),
            probe_granularity,
            id: None,
            n_produced: 0,
        }
    }
}

#[doc(hidden)]
impl<T, O> Kernel for LttngSource<T, O>
where
    T: Send + Clone + 'static,
    O: CpuBufferWriter<Item = T>,
{
    async fn init(&mut self, _mio: &mut MessageOutputs, meta: &mut BlockMeta) -> Result<()> {
        let s = meta.instance_name().unwrap();
        self.id = Some(s.split('_').next_back().unwrap().parse::<u64>().unwrap());
        Ok(())
    }

    async fn work(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let o = self.output.slice();
        o.fill(unsafe { std::mem::zeroed() });

        let before = self.n_produced / self.probe_granularity;
        let n = o.len();
        self.output.produce(n);
        self.n_produced += n as u64;
        let after = self.n_produced / self.probe_granularity;

        for i in 1..=(after - before) {
            tracepoints::futuresdr::tx(self.id.unwrap(), before + i);
        }

        Ok(())
    }
}
