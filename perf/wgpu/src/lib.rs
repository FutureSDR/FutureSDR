use anyhow::Result;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::blocks::Wgpu;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::wgpu;
use futuresdr::runtime::buffer::wgpu::D2HReader;
use futuresdr::runtime::buffer::wgpu::H2DWriter;
use std::iter::repeat_with;

#[cfg(not(target_arch = "wasm32"))]
use futuresdr::runtime::scheduler::SmolScheduler;

pub async fn run(run: u64, scheduler: String, samples: u64, buffer_size: u64) -> Result<()> {
    let orig: Vec<f32> = repeat_with(rand::random::<f32>)
        .take(samples as usize)
        .collect();

    let mut fg = Flowgraph::new();

    let src = VectorSource::<f32, H2DWriter<f32>>::new(orig.clone());
    let instance = wgpu::Instance::new().await;
    let mul = Wgpu::new(instance, buffer_size / 4, 4, 4);
    let snk = VectorSink::<f32, D2HReader<f32>>::new(1024);

    connect!(fg, src > mul > snk);

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

    let start = web_time::Instant::now();
    runtime.run_async(fg).await?;
    let elapsed = start.elapsed();

    let snk = snk.get()?;
    let v = snk.items();

    assert_eq!(v.len(), samples as usize);
    for i in 0..v.len() {
        assert!((orig[i] * 12.0 - v[i]).abs() < f32::EPSILON);
    }

    #[cfg(target_arch = "wasm32")]
    {
        leptos::logging::log!(
            "{},{},{},{},{}",
            run,
            scheduler,
            samples,
            buffer_size,
            elapsed.as_secs_f64()
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
