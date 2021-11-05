use clap::{value_t, App, Arg};
use lttng_ust::import_tracepoints;
use std::ptr;
use std::time;

use futuresdr::anyhow::{Context, Result};
use futuresdr::async_trait::async_trait;
use futuresdr::blocks::CopyRandBuilder;
use futuresdr::blocks::Head;
use futuresdr::runtime::scheduler::FlowScheduler;
use futuresdr::runtime::scheduler::SmolScheduler;
use futuresdr::runtime::scheduler::TpbScheduler;
use futuresdr::runtime::AsyncKernel;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::WorkIo;

import_tracepoints!(concat!(env!("OUT_DIR"), "/tracepoints.rs"), tracepoints);

const GRANULARITY: u64 = 32768;

fn main() -> Result<()> {
    let matches = App::new("Vect Rand Flowgraph")
        .arg(
            Arg::with_name("run")
                .short("r")
                .long("run")
                .takes_value(true)
                .value_name("RUN")
                .default_value("0")
                .help("Sets run number."),
        )
        .arg(
            Arg::with_name("stages")
                .short("s")
                .long("stages")
                .takes_value(true)
                .value_name("STAGES")
                .default_value("6")
                .help("Sets the number of stages."),
        )
        .arg(
            Arg::with_name("pipes")
                .short("p")
                .long("pipes")
                .takes_value(true)
                .value_name("PIPES")
                .default_value("5")
                .help("Sets the number of pipes."),
        )
        .arg(
            Arg::with_name("samples")
                .short("n")
                .long("samples")
                .takes_value(true)
                .value_name("SAMPLES")
                .default_value("15000000")
                .help("Sets the number of samples."),
        )
        .arg(
            Arg::with_name("max_copy")
                .short("m")
                .long("max_copy")
                .takes_value(true)
                .value_name("SAMPLES")
                .default_value("4000000000")
                .help("Sets the maximum number of samples to copy in one call to work()."),
        )
        .arg(
            Arg::with_name("scheduler")
                .short("S")
                .long("scheduler")
                .takes_value(true)
                .value_name("SCHEDULER")
                .default_value("smol1")
                .help("Sets the scheduler."),
        )
        .get_matches();

    let run = value_t!(matches.value_of("run"), u32).context("no run")?;
    let pipes = value_t!(matches.value_of("pipes"), u32).context("no pipe")?;
    let stages = value_t!(matches.value_of("stages"), u32).context("no stages")?;
    let samples = value_t!(matches.value_of("samples"), usize).context("no samples")?;
    let max_copy = value_t!(matches.value_of("max_copy"), usize).context("no max_copy")?;
    let scheduler = value_t!(matches.value_of("scheduler"), String).context("no scheduler")?;

    let mut fg = Flowgraph::new();

    let mut snks = Vec::new();

    for _ in 0..pipes {
        let src = fg.add_block(NullSourceLatency::new(4, GRANULARITY));
        let head = fg.add_block(Head::new(4, samples as u64));
        fg.connect_stream(src, "out", head, "in")?;

        let mut last = fg.add_block(CopyRandBuilder::new(4).max_copy(max_copy).build());
        fg.connect_stream(head, "out", last, "in")?;

        for _ in 1..stages {
            let block = fg.add_block(CopyRandBuilder::new(4).max_copy(max_copy).build());
            fg.connect_stream(last, "out", block, "in")?;
            last = block;
        }

        let snk = fg.add_block(NullSinkLatency::new(4, GRANULARITY));
        fg.connect_stream(last, "out", snk, "in")?;
        snks.push(snk);
    }

    let elapsed;

    if scheduler == "smol1" {
        let runtime = Runtime::custom(SmolScheduler::new(1, false)).build();
        let now = time::Instant::now();
        fg = runtime.run(fg)?;
        elapsed = now.elapsed();
    } else if scheduler == "smoln" {
        let runtime = Runtime::custom(SmolScheduler::default()).build();
        let now = time::Instant::now();
        fg = runtime.run(fg)?;
        elapsed = now.elapsed();
    } else if scheduler == "tpb" {
        let runtime = Runtime::custom(TpbScheduler::new()).build();
        let now = time::Instant::now();
        fg = runtime.run(fg)?;
        elapsed = now.elapsed();
    } else if scheduler == "flow" {
        let runtime = Runtime::custom(FlowScheduler::new()).build();
        let now = time::Instant::now();
        fg = runtime.run(fg)?;
        elapsed = now.elapsed();
    } else {
        panic!("unknown scheduler");
    }

    for s in snks {
        let snk = fg.block_async::<NullSinkLatency>(s).context("no block")?;
        let v = snk.n_received();
        assert_eq!(v, samples as u64);
    }

    println!(
        "{},{},{},{},{},{},{}",
        run,
        pipes,
        stages,
        samples,
        max_copy,
        scheduler,
        elapsed.as_secs_f64()
    );

    Ok(())
}

// =========================================================
// NULL SOURCE
// =========================================================
pub struct NullSourceLatency {
    item_size: usize,
    probe_granularity: u64,
    id: Option<u64>,
    n_produced: u64,
}

impl NullSourceLatency {
    pub fn new(item_size: usize, probe_granularity: u64) -> Block {
        Block::new_async(
            BlockMetaBuilder::new("NullSourceLatency").build(),
            StreamIoBuilder::new().add_output("out", item_size).build(),
            MessageIoBuilder::new().build(),
            NullSourceLatency {
                item_size,
                probe_granularity,
                id: None,
                n_produced: 0,
            },
        )
    }
}

#[async_trait]
impl AsyncKernel for NullSourceLatency {
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
        let o = sio.output(0).slice::<u8>();
        debug_assert_eq!(o.len() % self.item_size, 0);

        unsafe {
            ptr::write_bytes(o.as_mut_ptr(), 0, o.len());
        }

        let before = self.n_produced / self.probe_granularity;
        let n = o.len() / self.item_size;
        sio.output(0).produce(n);
        self.n_produced += n as u64;
        let after = self.n_produced / self.probe_granularity;

        if before != after {
            tracepoints::null_rand_latency::tx(self.id.unwrap(), after);
        }
        Ok(())
    }
}

// =========================================================
// NULL SINK
// =========================================================
pub struct NullSinkLatency {
    item_size: usize,
    n_received: u64,
    probe_granularity: u64,
    id: Option<u64>,
}

impl NullSinkLatency {
    pub fn new(item_size: usize, probe_granularity: u64) -> Block {
        Block::new_async(
            BlockMetaBuilder::new("NullSinkLatency").build(),
            StreamIoBuilder::new().add_input("in", item_size).build(),
            MessageIoBuilder::new().build(),
            NullSinkLatency {
                item_size,
                n_received: 0,
                probe_granularity,
                id: None,
            },
        )
    }

    pub fn n_received(&self) -> u64 {
        self.n_received
    }
}

#[async_trait]
impl AsyncKernel for NullSinkLatency {
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
        debug_assert_eq!(i.len() % self.item_size, 0);

        let before = self.n_received / self.probe_granularity;

        let n = i.len() / self.item_size;
        if n > 0 {
            self.n_received += n as u64;
            sio.input(0).consume(n);
        }

        if sio.input(0).finished() {
            io.finished = true;
        }

        let after = self.n_received / self.probe_granularity;
        if before ^ after != 0 {
            tracepoints::null_rand_latency::rx(self.id.unwrap(), after);
        }
        Ok(())
    }
}
