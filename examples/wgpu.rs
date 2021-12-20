use anyhow::Result;
use std::iter::repeat_with;
use instant;
use json;
use json::JsonValue;
//use std::fs::File;
//use std::io::prelude::*;

use futuresdr::blocks::{VectorSink};
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::blocks::VectorSourceBuilder;
use futuresdr::blocks::WgpuBuilderWasm;
use futuresdr::runtime::buffer::wgpu::WgpuBroker;
use futuresdr::runtime::buffer::wgpu;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

fn main() -> Result<()> {

    let mut buffer_values = Vec::new();
    buffer_values.push(2048);
    /*

    for i in i..10{
        buffer_values.push(i32::pow(2, i) * buffer_values[0]);
    }

 */


    //let mut performance_values = Vec::new();
  /*  for i in 1..3{
        performance_values.push(i * 1_000_000);
    }
    performance_values.clear();


    performance_values.push(1_000_000);
    */
    let mut times = json::JsonValue::new_object();

    let n_items = 3_000_000;


    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(n_items).collect();


    for n in buffer_values {
        let mut fg = Flowgraph::new();

        let start = instant::Instant::now();

        let wgpu_broker = pollster::block_on(WgpuBroker::new());

        let src = VectorSourceBuilder::<f32>::new(orig.clone()).build();
        let wgpu = WgpuBuilderWasm::new(wgpu_broker, n as u64).build();
        let snk = VectorSinkBuilder::<f32>::new().build();

        let src = fg.add_block(src);
        let wgpu = fg.add_block(wgpu);
        let snk = fg.add_block(snk);

        fg.connect_stream_with_type(src, "out", wgpu, "in", wgpu::H2D::new())?;
        fg.connect_stream_with_type(wgpu, "out", snk, "in", wgpu::D2H::new())?;

        fg = Runtime::new().run(fg)?;

        let snk = fg.block_async::<VectorSink<f32>>(snk).unwrap();
        let v = snk.items();

        let duration = start.elapsed();

        assert_eq!(v.len(), n_items);
        for i in 0..v.len() {
            assert!((orig[i]*12.0 - v[i]).abs() < f32::EPSILON*2.0);
        }
        times[(n).to_string()] = JsonValue::from(duration.as_millis() as u64);
    }

    log::info!("{:#}", times);

    /*let d = format!("JSON: {:#}", times);
    let mut file = File::create("foo.txt")?;
    file.write_all(d.as_bytes())?;

*/
    Ok(())
}
