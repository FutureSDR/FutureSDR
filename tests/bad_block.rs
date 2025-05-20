use anyhow::bail;
use anyhow::Result;
use futuresdr::async_io::block_on;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::blocks::Throttle;
use futuresdr::prelude::*;
use std::cmp;

pub enum FailType {
    Panic,
    Error,
}

/// Intentionally generate errors to test the runtime.
#[derive(Block)]
pub struct BadBlock<T: CpuSample> {
    pub work_fail: Option<FailType>,
    pub drop_fail: Option<FailType>,
    #[input]
    input: circular::Reader<T>,
    #[output]
    output: circular::Writer<T>,
}

impl<T: CpuSample> BadBlock<T> {
    pub fn new() -> Self {
        Self {
            work_fail: None,
            drop_fail: None,
            input: Default::default(),
            output: Default::default(),
        }
    }
}

impl<T: CpuSample> Default for BadBlock<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[doc(hidden)]
impl<T: CpuSample> Kernel for BadBlock<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        meta: &mut BlockMeta,
    ) -> Result<()> {
        match self.work_fail {
            Some(FailType::Panic) => {
                debug!("BadBlock::work() {:?} : panic", meta.instance_name());
                panic!("BadBlock!");
            }
            Some(FailType::Error) => {
                debug!("BadBlock! {:?} work(): Err", meta.instance_name());
                bail!("BadBlock!");
            }
            _ => {}
        }

        let i = self.input.slice();
        let o = self.output.slice();
        let i_len = i.len();

        let m = cmp::min(i_len, o.len());
        if m > 0 {
            o[..m].clone_from_slice(&i[..m]);
            self.input.consume(m);
            self.output.produce(m);
        }

        if self.input.finished() && m == i_len {
            io.finished = true;
        }

        Ok(())
    }
}

impl<T: CpuSample> Drop for BadBlock<T> {
    fn drop(&mut self) {
        debug!("In BadBlock::drop()");
        if let Some(FailType::Panic) = self.drop_fail {
            debug!("BadBlock! drop(): panic");
            panic!("BadBlock!");
        }
    }
}

enum RunMode {
    Run,
    Terminate,
}

fn run_badblock(bb: BadBlock<f32>, mode: RunMode) -> Result<Option<Error>> {
    let mut fg = Flowgraph::new();

    let null_source = NullSource::<f32>::new();
    let throttle = Throttle::<f32>::new(100.0);
    let head = Head::<f32>::new(10);
    let null_sink = NullSink::<f32>::new();

    connect!(fg, null_source > throttle > head > bb > null_sink);

    let rt_ret = match mode {
        RunMode::Run => Runtime::new().run(fg),
        RunMode::Terminate => {
            let rt = Runtime::new();
            let (fg_task, mut fg_handle) = rt.start_sync(fg);
            block_on(async move {
                // Sleep to allow work to be called at least once
                futuresdr::async_io::Timer::after(std::time::Duration::from_millis(1)).await;
                let _ = fg_handle.terminate().await;
                fg_task.await
            })
        }
    };
    //This will drop fg
    match rt_ret {
        Ok(_) => Ok(None),
        Err(e) => Ok(Some(e)),
    }
}

// //////////////////////////////////
// RunMode::Run

#[test]
fn run_no_err() -> Result<()> {
    let bb = BadBlock::<f32>::new();
    match run_badblock(bb, RunMode::Run)? {
        None => Ok(()),
        Some(e) => bail!("Expected None, got: {}", e),
    }
}

#[test]
fn run_work_err() -> Result<()> {
    let mut bb = BadBlock::<f32>::new();
    bb.work_fail = Some(FailType::Error);
    match run_badblock(bb, RunMode::Run)? {
        None => bail!("Expected Error, got: None"),
        Some(e) => {
            debug!("Error: {}", e);
            Ok(())
        }
    }
}

#[test]
#[ignore]
#[should_panic(expected = "BadBlock!")]
fn run_work_panic() {
    //FIXME: (#89) this currently hangs the runtime
    let mut bb = BadBlock::<f32>::new();
    bb.work_fail = Some(FailType::Panic);
    let _ = run_badblock(bb, RunMode::Run);
}

#[test]
#[should_panic(expected = "BadBlock!")]
fn run_drop_panic() {
    let mut bb = BadBlock::<f32>::new();
    bb.drop_fail = Some(FailType::Panic);
    let _ = run_badblock(bb, RunMode::Run);
}

// //////////////////////////////////
// RunMode::Terminate

#[test]
fn terminate_no_err() -> Result<()> {
    let bb = BadBlock::<f32>::new();
    match run_badblock(bb, RunMode::Terminate)? {
        None => Ok(()),
        Some(e) => bail!("Expected None, got: {}", e),
    }
}

/// BadBlock returns work error, terminate msg is sent later.
#[test]
fn terminate_work_err() -> Result<()> {
    // panics `Err` value: send failed because receiver is gone
    // FIXME: should probably return some sort of flowgraph not running error
    let mut bb = BadBlock::<f32>::new();
    bb.work_fail = Some(FailType::Error);
    match run_badblock(bb, RunMode::Terminate)? {
        None => bail!("Expected Error, got: None"),
        Some(e) => {
            debug!("Error: {}", e);
            Ok(())
        }
    }
}

#[test]
#[ignore]
// #[should_panic(expected = "BadBlock!")]
fn terminate_work_panic() -> Result<()> {
    // This sometimes returns a flowgraph terminated error instead of panicking.
    // Other times it panics in various channel/scheduler locations (send or drop)
    // Assume race condition.
    // TODO: can we do *something* reliably here?
    let mut bb = BadBlock::<f32>::new();
    bb.work_fail = Some(FailType::Panic);
    match run_badblock(bb, RunMode::Terminate)? {
        None => bail!("Expected Error, got: None"),
        Some(e) => {
            debug!("Error: {}", e);
            if e.to_string() != "Flowgraph was terminated" {
                bail!("Unexpected Error: {}", e)
            }
            Ok(())
        }
    }
}

#[test]
#[should_panic(expected = "BadBlock!")]
fn terminate_drop_panic() {
    //TODO: try to make consistent
    //      Intermittently panics with "task has failed", sometimes Error("Flowgraph was terminated")
    //      Assume race condition.
    let mut bb = BadBlock::<f32>::new();
    bb.drop_fail = Some(FailType::Panic);
    match run_badblock(bb, RunMode::Terminate) {
        Ok(None) => panic!("Expected Error, got: None"),
        Ok(Some(e)) => {
            debug!("Error: {}", e);
            if e.to_string() != "Flowgraph was terminated" {
                panic!("Unexpected Error: {e}")
            }
        }
        Err(e) => panic!("Unexpected Error: {e}"),
    }
}
