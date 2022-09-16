use std::iter::repeat_with;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use futuresdr::anyhow::{Context, Result};
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::blocks::Wgpu;
use futuresdr::log::info;
use futuresdr::runtime::buffer::wgpu;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

#[cfg(not(target_arch = "wasm32"))]
use futuresdr::runtime::scheduler::SmolScheduler;

#[cfg(target_arch = "wasm32")]
use web_sys::console;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn run_fg(run: u64, samples: u64, buffer_size: u64) {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    crate::run(run, "wasm".to_string(), samples, buffer_size)
        .await
        .unwrap();
}

pub async fn run(run: u64, scheduler: String, samples: u64, buffer_size: u64) -> Result<()> {
    info!("start flowgraph");

    let orig: Vec<f32> = repeat_with(rand::random::<f32>)
        .take(samples as usize)
        .collect();

    let mut fg = Flowgraph::new();
    let broker = wgpu::Broker::new().await;

    let src = VectorSource::<f32>::new(orig.clone());
    let mul = Wgpu::new(broker, buffer_size / 4, 2, 2);
    let snk = VectorSink::<f32>::new(samples as usize);

    let src = fg.add_block(src);
    let mul = fg.add_block(mul);
    let snk = fg.add_block(snk);

    fg.connect_stream_with_type(src, "out", mul, "in", wgpu::H2D::new())?;
    fg.connect_stream_with_type(mul, "out", snk, "in", wgpu::D2H::new())?;

    info!("start flowgraph");

    let runtime;

    #[cfg(target_arch = "wasm32")]
    {
        runtime = Runtime::new();
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if scheduler == "smol1" {
            runtime = Runtime::with_scheduler(SmolScheduler::new(1, false));
        } else if scheduler == "smoln" {
            runtime = Runtime::with_scheduler(SmolScheduler::default());
        } else {
            panic!("scheduler not supported");
        }
    }

    let start = instant::Instant::now();
    let fg = runtime.run_async(fg).await?;
    let elapsed = start.elapsed();

    let snk = fg
        .kernel::<VectorSink<f32>>(snk)
        .context("wrong block type")?;
    let v = snk.items();

    assert_eq!(v.len(), samples as usize);
    for i in 0..v.len() {
        assert!((orig[i] * 12.0 - v[i]).abs() < f32::EPSILON);
    }

    #[cfg(target_arch = "wasm32")]
    {
        console::log_1(
            &format!(
                "{},{},{},{},{}",
                run,
                scheduler,
                samples,
                buffer_size,
                elapsed.as_secs_f64()
            )
            .into(),
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        println!(
            "{},{},{},{},{}",
            run,
            scheduler,
            samples,
            buffer_size,
            elapsed.as_secs_f64()
        );
    }
    Ok(())
}
