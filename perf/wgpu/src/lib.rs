use clap::{value_t, App, Arg};
#[cfg(not(target_arch = "wasm32"))]
use std::fs::File;
#[cfg(not(target_arch = "wasm32"))]
use std::io::prelude::*;
use std::iter::repeat_with;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use futuresdr::anyhow::{Context, Result};
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSourceBuilder;
use futuresdr::blocks::Wgpu;
use futuresdr::log::info;
use futuresdr::runtime::buffer::wgpu;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

#[cfg(not(target_arch = "wasm32"))]
use futuresdr::runtime::scheduler::SmolScheduler;
#[cfg(target_arch = "wasm32")]
use futuresdr::runtime::scheduler::WasmScheduler;
use instant;
use json;
use json::JsonValue;
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

#[cfg(target_arch = "wasm32")]
extern crate web_sys;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn run_fg() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init().expect("could not initialize logger");
    run().await.unwrap();
}

pub async fn run() -> Result<()> {
    let mut run: u64 = 1;
    let mut scheduler: String = String::from("smol1");
    let mut items: usize = 1_000_000;
    let mut buffersize: u64 = 4096;

    get_commandline_args(&mut run, &mut scheduler, &mut items, &mut buffersize);

    info!("start flowgraph");

    let mut times = json::JsonValue::new_object();

    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(items).collect();
    for r in 0..run {
        let mut fg = Flowgraph::new();

        let src = VectorSourceBuilder::<f32>::new(orig.clone()).build();
        let broker = wgpu::Broker::new().await;
        let mul = Wgpu::new(broker, buffersize, 3, 2);
        let snk = VectorSink::<f32>::new(8192);

        let src = fg.add_block(src);
        let mul = fg.add_block(mul);
        let snk = fg.add_block(snk);

        fg.connect_stream_with_type(src, "out", mul, "in", wgpu::H2D::new())?;
        fg.connect_stream_with_type(mul, "out", snk, "in", wgpu::D2H::new())?;

        info!("start flowgraph");

        let runtime = get_runtime(scheduler.clone());

        let start = instant::Instant::now();
        fg = runtime.run_async(fg).await?;
        let duration = start.elapsed();

        let snk = fg
            .block_async::<VectorSink<f32>>(snk)
            .context("wrong block type")?;
        let v = snk.items();

        assert_eq!(v.len(), items);
        for i in 0..v.len() {
            assert!((orig[i] * 12.0 - v[i]).abs() < f32::EPSILON);
        }
        info!("end flowgraph");
        times[(r.to_string())] = JsonValue::from(duration.as_micros() as u64);
    }
    let s = format!("{:#}", times);
    log_output(s);

    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn log_output(s: String) {
    web_sys::console::log_1(&s.into());
}

#[cfg(not(target_arch = "wasm32"))]
pub fn log_output(s: String) {
    if !Path::new("output.txt").exists() {
        let _res = File::create("output.txt");
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .append(true)
        .open("output.txt")
        .unwrap();
    let _t = file.write_all(s.as_bytes());
}

#[cfg(not(target_arch = "wasm32"))]
fn get_runtime(scheduler: String) -> Runtime<SmolScheduler> {
    if scheduler == "smol1" {
        return Runtime::with_scheduler(SmolScheduler::new(1, false));
    } else if scheduler == "smoln" {
        return Runtime::with_scheduler(SmolScheduler::default());
    } else {
        info!("no valid scheduler - using default smol1");
        return Runtime::with_scheduler(SmolScheduler::new(1, false));
    }
}

#[cfg(target_arch = "wasm32")]
fn get_runtime(_: String) -> Runtime<WasmScheduler> {
    Runtime::new()
}

fn get_commandline_args(
    run: &mut u64,
    scheduler: &mut String,
    items: &mut usize,
    buffersize: &mut u64,
) {
    let matches = App::new("FIR Rand Flowgraph")
        .arg(
            Arg::with_name("run")
                .short("r")
                .long("run")
                .takes_value(true)
                .value_name("RUN")
                .default_value("1")
                .help("Sets run number."),
        )
        .arg(
            Arg::with_name("scheduler")
                .short("S")
                .long("scheduler")
                .takes_value(true)
                .value_name("SCHEDULER")
                .default_value("smol1")
                .help("Sets the scheduler."),
        )
        .arg(
            Arg::with_name("items")
                .short("i")
                .long("items")
                .takes_value(true)
                .value_name("ITEMS")
                .default_value("1000000")
                .help("Sets item amount."),
        )
        .arg(
            Arg::with_name("buffersize")
                .short("bs")
                .long("buffersize")
                .takes_value(true)
                .value_name("BUFFERSIZE")
                .default_value("4096")
                .help("Sets buffer size."),
        )
        .get_matches();

    *run = value_t!(matches.value_of("run"), u64)
        .context("no run")
        .unwrap();
    *scheduler = value_t!(matches.value_of("scheduler"), String)
        .context("no scheduler")
        .unwrap();
    *items = value_t!(matches.value_of("items"), usize)
        .context("no items")
        .unwrap();
    *buffersize = value_t!(matches.value_of("buffersize"), u64)
        .context("no buffersize")
        .unwrap();
}
