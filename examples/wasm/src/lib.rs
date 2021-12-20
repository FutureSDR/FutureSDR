use std::iter::repeat_with;
use wasm_bindgen::prelude::*;
use instant;
use json;
use json::JsonValue;


use futuresdr::blocks::{WgpuBuilderWasm};
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::blocks::VectorSourceBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::buffer::wgpu;
use futuresdr::runtime::buffer::wgpu::WgpuBroker;

extern crate web_sys;

#[wasm_bindgen]
pub async fn run_fg() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init().expect("could not initialize logger");

    let mut performance_values = Vec::new();
    for i in 1..10{
        performance_values.push(i * 1_000_000);
    }
    //log::info!("{:?}",performance_values);
    //let performance_values = vec!(100, 10_000, 100_000, 1_000_000, 5_000_000, 10_000_000);
    // let performance_values = vec!( 1_000_000 );
    let mut times = json::JsonValue::new_object();

    let mut buffer_values = Vec::new();
    let buffer_size = 2048;
    for i in 0..9{
        buffer_values.push(i32::pow(2, i) * buffer_size);
    }
    buffer_values.clear();
    buffer_values.push(4096);
    let n_items = 1_000_000;

    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(n_items).collect();

    for n in buffer_values {
        log::info!("starting");
        let mut fg = Flowgraph::new();



        let start = instant::Instant::now();

        let wgpu_broker = WgpuBroker::new().await;

        let src = VectorSourceBuilder::<f32>::new(orig.clone()).build();
        let wgpu = WgpuBuilderWasm::new(wgpu_broker, n as u64).build();
        let snk = VectorSinkBuilder::<f32>::new().build();

        let src = fg.add_block(src);
        let wgpu = fg.add_block(wgpu);
        //  let apply = Apply::new(|i: &f32| -> f32 { *i * 12.0 });
        let snk = fg.add_block(snk);

        fg.connect_stream_with_type(src, "out", wgpu, "in", wgpu::H2D::new()).unwrap();
        // fg.connect_stream(src, "out", wgpu, "in").unwrap();
        fg.connect_stream_with_type(wgpu, "out", snk, "in", wgpu::D2H::new()).unwrap();

        log::info!("*** start runtime  ***");
        fg = Runtime::new().run(fg).await.unwrap();

        log::info!("*** flowgraph finished ***");
        let snk = fg.block_async::<VectorSink<f32>>(snk).unwrap();
        let v = snk.items();

        assert_eq!(v.len(), n_items);
        let duration = start.elapsed();
        for i in 0..v.len() {
/*
            if i >= 8192 && i <= (8192+n) as usize {

                continue;
            }

 */




            if i == 0 {
                log::info!("wrong: i {}  orig {}  expected res {}   res {}", 0, orig[0], orig[0] * 12.0 , v[0]);
            }


            if (orig[i] * 12.0 - v[i]).abs() > f32::EPSILON {
                log::info!("***********+");
                log::info!("output wrong: i {}  orig {}  expected res {}   res {}", i, orig[i], orig[i] * 12.0 , v[i]);
                log::info!("output wrong: i {}  orig {}  expected res {}   res {}", i+1, orig[i+1], orig[i+1] * 12.0 , v[i+1]);
                // log::info!("output wrong: i {}  orig {}   res {}", i+1, orig[i+1] * 12.0, v[i+1]);
                panic!("wrong data");
            }
        }

        log::info!("FINISHED YAY!");


        log::info!("Duration for {} elements:   {}ms", v.len(), duration.as_millis());
        times[(n.to_string())] = JsonValue::from(duration.as_millis() as u64);
    }

    log::info!("JSON: \n {:#}", times);




}
