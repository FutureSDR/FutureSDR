use async_io::block_on;
use async_io::Timer;
use std::time::Duration;

use futuresdr::anyhow::Result;
use futuresdr::blocks::MessageSourceBuilder;
use futuresdr::runtime::scheduler::FlowScheduler;
use futuresdr::runtime::scheduler::Scheduler;
use futuresdr::runtime::scheduler::SmolScheduler;
use futuresdr::runtime::scheduler::TpbScheduler;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;

fn create_fg() -> Flowgraph {
    let mut fg = Flowgraph::new();
    let src = MessageSourceBuilder::new(Pmt::U32(123), Duration::from_millis(100))
        .n_messages(20)
        .build();
    fg.add_block(src);
    fg
}

fn main() -> Result<()> {
    Runtime::new().run(create_fg())?;

    let rt = Runtime::new();

    let (h1, _) = block_on(rt.start(create_fg()));
    let (h2, _) = block_on(rt.start(create_fg()));

    block_on(h1)?;
    block_on(h2)?;

    let rt1 = Runtime::new();
    let rt2 = Runtime::new();

    let (h1, _) = block_on(rt1.start(create_fg()));
    let (h2, _) = block_on(rt2.start(create_fg()));

    block_on(h1)?;
    block_on(h2)?;

    let f1 = FlowScheduler::new();
    let f2 = FlowScheduler::new();
    let f3 = FlowScheduler::new();

    f1.spawn(async {
        Timer::after(Duration::from_secs(1)).await;
        println!("timer f1");
    })
    .detach();
    f2.spawn(async {
        Timer::after(Duration::from_secs(1)).await;
        println!("timer f2");
    })
    .detach();
    f3.spawn(async {
        Timer::after(Duration::from_secs(1)).await;
        println!("timer f3");
    })
    .detach();

    drop(f1);
    drop(f2);
    std::thread::sleep(Duration::from_secs(2));
    drop(f3);

    let t1 = TpbScheduler::new();
    let t2 = TpbScheduler::new();
    let t3 = TpbScheduler::new();

    t1.spawn(async {
        Timer::after(Duration::from_secs(1)).await;
        println!("timer t1");
    })
    .detach();
    t2.spawn(async {
        Timer::after(Duration::from_secs(1)).await;
        println!("timer t2");
    })
    .detach();
    t3.spawn(async {
        Timer::after(Duration::from_secs(1)).await;
        println!("timer t3");
    })
    .detach();

    drop(t1);
    drop(t2);
    std::thread::sleep(Duration::from_secs(2));
    drop(t3);

    let s1 = SmolScheduler::new(5, false);
    let s1c = s1.clone();

    s1.spawn(async {
        Timer::after(Duration::from_secs(1)).await;
        println!("timer s1");
    })
    .detach();
    s1c.spawn(async {
        Timer::after(Duration::from_secs(1)).await;
        println!("timer s1c");
    })
    .detach();

    std::thread::sleep(Duration::from_secs(2));
    drop(s1);
    drop(s1c);

    let s1 = SmolScheduler::new(5, false);
    let s2 = SmolScheduler::new(5, false);

    s1.spawn(async {
        Timer::after(Duration::from_secs(1)).await;
        println!("timer s1");
    })
    .detach();
    s2.spawn(async {
        Timer::after(Duration::from_secs(1)).await;
        println!("timer s2");
    })
    .detach();

    std::thread::sleep(Duration::from_secs(2));
    drop(s1);
    drop(s2);

    let s1 = SmolScheduler::new(5, false);
    let s2 = SmolScheduler::new(5, false);
    s1.spawn(async {
        Timer::after(Duration::from_secs(1)).await;
        println!("timer s1");
    })
    .detach();
    s2.spawn(async {
        Timer::after(Duration::from_secs(1)).await;
        println!("timer s2");
    })
    .detach();
    drop(s1);
    drop(s2);

    Ok(())
}
