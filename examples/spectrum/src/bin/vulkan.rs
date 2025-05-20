use anyhow::Result;
use futuresdr::blocks::seify::Builder;
use futuresdr::blocks::Apply;
use futuresdr::blocks::Fft;
use futuresdr::blocks::MovingAvg;
use futuresdr::blocks::Vulkan;
use futuresdr::blocks::WebsocketSinkBuilder;
use futuresdr::blocks::WebsocketSinkMode;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::vulkan;
use futuresdr::runtime::buffer::vulkan::Instance;

const FFT_SIZE: usize = 4096;

mod cs {
    vulkano_shaders::shader! {
        ty: "compute",
        src: "
#version 450

layout(local_size_x = 32, local_size_y = 1, local_size_z = 1) in;

layout(set = 0, binding = 0) buffer Data {
    float data[];
} buf;

void main() {
    uint idx = gl_GlobalInvocationID.x;
    buf.data[idx] = 4.3429448190325175 * log(buf.data[idx]);
}"
    }
}

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let instance = Instance::new();
    let entry_point = cs::load(instance.device())
        .unwrap()
        .entry_point("main")
        .unwrap();

    let src = Builder::new("")?
        .frequency(100e6)
        .sample_rate(3.2e6)
        .gain(34.0)
        .build_source()?;

    let fft: Fft = Fft::with_options(
        FFT_SIZE,
        futuresdr::blocks::FftDirection::Forward,
        true,
        None,
    );

    let mut power = Apply::<_, _, _, _, vulkan::H2DWriter<f32>>::new(|x: &Complex32| x.norm_sqr());
    let log = Vulkan::new(instance.clone(), entry_point, 32);
    let keep = MovingAvg::<FFT_SIZE, vulkan::D2HReader<f32>>::new(0.1, 3);

    for _ in 0..4 {
        let buffer = instance.create_buffer(4096 * 4 * 8)?;
        power.output().add_buffer(buffer);
    }

    let snk = WebsocketSinkBuilder::<f32>::new(9001)
        .mode(WebsocketSinkMode::FixedBlocking(FFT_SIZE))
        .build();

    connect!(fg, src.outputs[0] > fft > power > log > keep > snk);
    connect!(fg, power < keep);

    Runtime::new().run(fg)?;
    Ok(())
}
