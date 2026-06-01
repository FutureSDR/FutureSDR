use anyhow::Result;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::blocks::Wgpu;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::wgpu;
use futuresdr::runtime::buffer::wgpu::D2HReader;
use futuresdr::runtime::buffer::wgpu::H2DWriter;
#[cfg(target_arch = "wasm32")]
use futuresdr::runtime::scheduler::WasmScheduler;
use std::iter::repeat_with;

pub async fn run() {
    run_inner().await.unwrap()
}

async fn run_inner() -> Result<()> {
    let n_items = 123123;
    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(n_items).collect();

    let mut fg = Flowgraph::new();

    #[cfg(not(target_arch = "wasm32"))]
    let snk = {
        let instance = wgpu::Instance::new().await;
        build_flowgraph(&mut fg, orig.clone(), instance)?
    };
    #[cfg(target_arch = "wasm32")]
    let snk = build_flowgraph(&mut fg, orig.clone()).await?;

    info!("start flowgraph");
    #[cfg(target_arch = "wasm32")]
    let fg = Runtime::with_scheduler(WasmScheduler::new(1))
        .run_async(fg)
        .await?;
    #[cfg(not(target_arch = "wasm32"))]
    let fg = Runtime::new().run_async(fg).await?;

    let v = fg.with_async(&snk, |snk| snk.items().clone()).await?;

    assert_eq!(v.len(), n_items);
    for i in 0..v.len() {
        assert!((orig[i] * 12.0 - v[i]).abs() < 5.0 * f32::EPSILON);
    }

    info!("data matches");
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn build_flowgraph(
    fg: &mut Flowgraph,
    orig: Vec<f32>,
    instance: wgpu::Instance,
) -> Result<BlockRef<VectorSink<f32, D2HReader<f32>>>> {
    let src = VectorSource::<f32, H2DWriter<f32>>::new(orig);
    let mul = Wgpu::new(instance, 4096, 4, 4);
    let snk = VectorSink::<f32, D2HReader<f32>>::new(1024);

    connect!(fg, src > mul > snk);

    Ok(snk)
}

#[cfg(target_arch = "wasm32")]
async fn build_flowgraph(
    fg: &mut Flowgraph,
    orig: Vec<f32>,
) -> Result<BlockRef<VectorSink<f32, D2HReader<f32>>>> {
    let local = fg.local_domain()?;
    Ok(fg
        .domain_run_async(local, async move |ctx: &LocalDomainContext<'_>| {
            let instance = wgpu::Instance::new().await;
            let src = ctx.add(VectorSource::<f32, H2DWriter<f32>>::new(orig));
            let mul = ctx.add(Wgpu::new(instance, 4096, 4, 4));
            let snk = ctx.add(VectorSink::<f32, D2HReader<f32>>::new(1024));

            connect_async!(ctx, src ~> mul ~> snk);

            Ok(snk)
        })
        .await?)
}
