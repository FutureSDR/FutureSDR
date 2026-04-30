use anyhow::Result;
use futuresdr::blocks::Copy;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::blocks::Throttle;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;
use futuresdr::runtime::dev::prelude::*;
use futuresdr::runtime::scheduler::FlowScheduler;
use std::iter::repeat_with;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

#[derive(Block)]
#[message_inputs(r#in)]
struct StopOnMessage {
    terminated: Arc<AtomicBool>,
}

impl StopOnMessage {
    fn new(terminated: Arc<AtomicBool>) -> Self {
        Self { terminated }
    }

    async fn r#in(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        _p: Pmt,
    ) -> futuresdr::runtime::Result<Pmt> {
        io.finished = true;
        Ok(Pmt::Ok)
    }
}

impl Kernel for StopOnMessage {}

impl Drop for StopOnMessage {
    fn drop(&mut self) {
        self.terminated.store(true, Ordering::SeqCst);
    }
}

#[test]
fn flowgraph() -> Result<()> {
    let mut fg = Flowgraph::new();

    let copy = Copy::<f32>::new();
    let head = Head::<f32>::new(1_000_000);
    let src = NullSource::<f32>::new();
    let snk = VectorSink::<f32>::new(1_000_000);

    connect!(fg, src > head > copy > snk);

    let fg = Runtime::new().run(fg)?;

    let snk = fg.block(&snk)?;
    let v = snk.items();

    assert_eq!(v.len(), 1_000_000);
    for i in v {
        assert!(i.abs() < f32::EPSILON);
    }

    Ok(())
}

#[test]
fn flowgraph_flow() -> Result<()> {
    let mut fg = Flowgraph::new();

    let copy = Copy::<f32>::new();
    let head = Head::<f32>::new(1_000_000);
    let src = NullSource::<f32>::new();
    let snk = VectorSink::<f32>::new(1_000_000);

    connect!(fg, src > head > copy > snk);

    let fg = Runtime::with_scheduler(FlowScheduler::new()).run(fg)?;

    let snk = fg.block(&snk)?;
    let v = snk.items();

    assert_eq!(v.len(), 1_000_000);
    for i in v {
        assert!(i.abs() < f32::EPSILON);
    }

    Ok(())
}

#[test]
fn fg_terminate() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = NullSource::<f32>::new();
    let throttle = Throttle::<f32>::new(10.0);
    let snk = NullSink::<f32>::new();

    connect!(fg, src > throttle > snk);

    let rt = Runtime::new();
    let running = rt.start(fg)?;
    Runtime::block_on(async move {
        Timer::after(std::time::Duration::from_secs(1)).await;
        running.stop().await.unwrap();
        let _ = running.wait().await;
    });

    Ok(())
}

#[test]
fn fg_handle_survives_runtime_and_task_drop() -> Result<()> {
    let mut fg = Flowgraph::new();
    let terminated = Arc::new(AtomicBool::new(false));
    let blk = fg.add(StopOnMessage::new(terminated.clone()));

    let runtime = Runtime::new();
    let running = runtime.start(fg)?;
    let (task, handle) = running.split();

    drop(task);
    drop(runtime);

    Runtime::block_on(async move {
        handle.post(blk, "in", Pmt::Null).await?;

        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if terminated.load(Ordering::SeqCst) {
                return Ok(());
            }

            assert!(
                Instant::now() < deadline,
                "flowgraph did not terminate within 1 second"
            );
            Timer::after(Duration::from_millis(10)).await;
        }
    })
}

#[test]
fn fg_rand_vec() -> Result<()> {
    let mut fg = Flowgraph::new();

    let n_items = 10_000_000;
    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(n_items).collect();

    let src = VectorSource::<f32>::new(orig.clone());
    let copy = Copy::<f32>::new();
    let snk = VectorSink::<f32>::new(n_items);

    connect!(fg, src > copy > snk);

    let fg = Runtime::new().run(fg)?;

    let snk = fg.block(&snk)?;
    let v = snk.items();

    assert_eq!(v.len(), n_items);
    for i in 0..v.len() {
        assert!((orig[i] - v[i]).abs() < f32::EPSILON);
    }

    Ok(())
}

#[test]
fn fg_rand_vec_multi_snk() -> Result<()> {
    let mut fg = Flowgraph::new();

    let n_items = 1_000_000;
    let n_snks = 10;
    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(n_items).collect();

    let src = VectorSource::<f32>::new(orig.clone());
    let copy = Copy::<f32>::new();

    connect!(fg, src > copy);

    let mut snks = Vec::new();
    for _ in 0..n_snks {
        let snk = VectorSink::<f32>::new(n_items);
        connect!(fg, copy > snk);
        snks.push(snk);
    }

    let fg = Runtime::new().run(fg)?;

    for s in &snks {
        let snk = fg.block(s)?;
        let v = snk.items();

        assert_eq!(v.len(), n_items);
        for i in 0..v.len() {
            assert!((orig[i] - v[i]).abs() < f32::EPSILON);
        }
    }

    Ok(())
}
#[test]
fn flowgraph_instance_name() -> Result<()> {
    let rt = Runtime::new();
    let name = "my_special_name";
    let mut fg = Flowgraph::new();

    let src = NullSource::<f32>::new();
    let snk = NullSink::<f32>::new();
    connect!(fg, src > snk);
    fg.block_mut(&snk)?.set_instance_name(name);
    let fg = rt.start(fg)?.handle();

    let desc = Runtime::block_on(async move { fg.describe().await })?;
    assert_eq!(desc.blocks.first().unwrap().instance_name, name);
    Ok(())
}
