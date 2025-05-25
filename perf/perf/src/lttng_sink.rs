use futuresdr::prelude::*;
use lttng_ust::import_tracepoints;

import_tracepoints!(concat!(env!("OUT_DIR"), "/tracepoints.rs"), tracepoints);

/// Null sink that calls an [lttng](https://lttng.org/) tracepoint for every batch of received samples.
#[derive(Block)]
pub struct LttngSink<T, I = DefaultCpuReader<T>>
where
    T: Send + 'static,
    I: CpuBufferReader<Item = T>,
{
    #[input]
    input: I,
    n_received: u64,
    probe_granularity: u64,
    id: Option<u64>,
}

impl<T, I> LttngSink<T, I>
where
    T: Send + 'static,
    I: CpuBufferReader<Item = T>,
{
    /// Create LttngSink block
    pub fn new(probe_granularity: u64) -> Self {
        Self {
            input: I::default(),
            n_received: 0,
            probe_granularity,
            id: None,
        }
    }
    /// Get number of received samples
    pub fn n_received(&self) -> u64 {
        self.n_received
    }
}

#[doc(hidden)]
impl<T, I> Kernel for LttngSink<T, I>
where
    T: Send + 'static,
    I: CpuBufferReader<Item = T>,
{
    async fn init(&mut self, _mio: &mut MessageOutputs, meta: &mut BlockMeta) -> Result<()> {
        let s = meta.instance_name().unwrap();
        self.id = Some(s.split('-').next_back().unwrap().parse::<u64>().unwrap());
        Ok(())
    }

    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = self.input.slice();
        let before = self.n_received / self.probe_granularity;

        let n = i.len();
        if n > 0 {
            self.n_received += n as u64;
            self.input.consume(n);
        }

        if self.input.finished() {
            io.finished = true;
        }

        let after = self.n_received / self.probe_granularity;
        if before != after {
            tracepoints::futuresdr::rx(self.id.unwrap(), after);
        }
        Ok(())
    }
}
