use futuresdr::{
    anyhow::{bail, Error, Result},
    async_io::block_on,
    blocks::{bad_block::*, Head, NullSink, NullSource, Throttle},
    runtime::{Flowgraph, Runtime},
};
use futuresdr_macros::connect;
use log::debug;

enum RunMode {
    Run,
    Terminate,
    TermRtDrop,
}

fn run_badblock(bb: BadBlock<f32>, mode: RunMode) -> Result<Option<Error>> {
    let mut fg = Flowgraph::new();

    let null_source = NullSource::<f32>::new();
    let throttle = Throttle::<f32>::new(100.0);
    let head = Head::<f32>::new(10);
    let null_sink = NullSink::<f32>::new();

    let bb = bb.to_block();

    connect!(fg, null_source > throttle > head > bb > null_sink);

    let rt_ret = match mode {
        RunMode::Run => Runtime::new().run(fg),
        RunMode::Terminate => {
            let rt = Runtime::new();
            let (fg_task, mut fg_handle) = block_on(rt.start(fg));
            block_on(async move {
                // Sleep to allow work to be called at least once
                futuresdr::async_io::Timer::after(std::time::Duration::from_millis(1)).await;
                fg_handle.terminate().await.unwrap();
                fg_task.await
            })
        }
        RunMode::TermRtDrop => {
            // This drops runtime before flowgraph and causes a deadlock regardless of any badblock errors.
            // E.g. `let (task, mut fg_handle) = block_on(Runtime::new().start(fg));`
            let fg_task;
            let mut fg_handle;
            {
                let rt = Runtime::new();
                (fg_task, fg_handle) = block_on(rt.start(fg));
            }
            block_on(async move {
                // Sleep to allow work to be called at least once
                futuresdr::async_io::Timer::after(std::time::Duration::from_millis(1)).await;
                fg_handle.terminate().await.unwrap();
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
    let bb = BadBlock::<f32>::default();
    match run_badblock(bb, RunMode::Run)? {
        None => Ok(()),
        Some(e) => bail!("Expected None, got: {}", e),
    }
}

#[test]
fn run_work_err() -> Result<()> {
    let mut bb = BadBlock::<f32>::default();
    bb.work_fail = Some(FailType::Error);
    match run_badblock(bb, RunMode::Run)? {
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
#[ignore]
#[should_panic(expected = "BadBlock!")]
fn run_work_panic() {
    //FIXME: (#89) this currently hangs the runtime
    let mut bb = BadBlock::<f32>::default();
    bb.work_fail = Some(FailType::Panic);
    let _ = run_badblock(bb, RunMode::Run);
}

#[test]
#[should_panic(expected = "BadBlock!")]
fn run_drop_panic() {
    let mut bb = BadBlock::<f32>::default();
    bb.drop_fail = Some(FailType::Panic);
    let _ = run_badblock(bb, RunMode::Run);
}

// //////////////////////////////////
// RunMode::Terminate

#[test]
#[ignore]
fn terminate_no_err() -> Result<()> {
    //FIXME: (#80) this should return OK but currently returns Error("Flowgraph was terminated")
    let bb = BadBlock::<f32>::default();
    match run_badblock(bb, RunMode::Terminate)? {
        None => Ok(()),
        Some(e) => bail!("Expected None, got: {}", e),
    }
}

/// BadBlock returns work error, terminate msg is sent later.
#[test]
#[ignore]
fn terminate_work_err() -> Result<()> {
    // panics `Err` value: send failed because receiver is gone
    // FIXME: should probably return some sort of flowgraph not running error
    let mut bb = BadBlock::<f32>::default();
    bb.work_fail = Some(FailType::Error);
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
#[ignore]
// #[should_panic(expected = "BadBlock!")]
fn terminate_work_panic() -> Result<()> {
    // This sometimes returns a flowgraph terminated error instead of panicking.
    // Other times it panics in various channel/scheduler locations (send or drop)
    // Assume race condition.
    // TODO: can we do *something* reliably here?
    let mut bb = BadBlock::<f32>::default();
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
#[ignore]
#[should_panic(expected = "BadBlock!")]
fn terminate_drop_panic() {
    //TODO: try to make consistent
    //      Intermittently panics with "task has failed", sometimes Error("Flowgraph was terminated")
    //      Assume race condition.
    let mut bb = BadBlock::<f32>::default();
    bb.drop_fail = Some(FailType::Panic);
    match run_badblock(bb, RunMode::Terminate) {
        Ok(None) => panic!("Expected Error, got: None"),
        Ok(Some(e)) => {
            debug!("Error: {}", e);
            if e.to_string() != "Flowgraph was terminated" {
                panic!("Unexpected Error: {}", e)
            }
        }
        Err(e) => panic!("Unexpected Error: {}", e),
    }
}

// //////////////////////////////////
// RunMode::TermRtDrop

#[test]
#[ignore]
fn rtdrop_no_err() -> Result<()> {
    //FIXME: (#89) this currently hangs (or panics with deadlock detected)
    let bb = BadBlock::<f32>::default();
    match run_badblock(bb, RunMode::TermRtDrop)? {
        None => Ok(()),
        Some(e) => bail!("Expected None, got: {}", e),
    }
}
