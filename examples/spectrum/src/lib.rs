use std::mem::size_of;

use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::blocks::Apply;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::SyncKernel;
use futuresdr::runtime::WorkIo;

pub fn lin2db_block() -> Block {
    Apply::new(|x: &f32| 10.0 * x.log10())
}

pub fn power_block() -> Block {
    Apply::new(|x: &Complex32| x.norm())
}

pub struct FftShift {}

impl FftShift {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Block {
        Block::new_sync(
            BlockMetaBuilder::new("FftShift").build(),
            StreamIoBuilder::new()
                .add_input("in", size_of::<f32>())
                .add_output("out", size_of::<f32>())
                .build(),
            MessageIoBuilder::new().build(),
            Self {},
        )
    }
}

#[async_trait]
impl SyncKernel for FftShift {
    fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let input = sio.input(0).slice::<f32>();
        let output = sio.output(0).slice::<f32>();

        let n = std::cmp::min(input.len(), output.len()) / 2048;

        for i in 0..n {
            for k in 0..2048 {
                let m = (k + 1024) % 2048;
                output[i * 2048 + m] = input[i * 2048 + k]
            }
        }

        if sio.input(0).finished() && n == input.len() / 2048 {
            io.finished = true;
        }

        sio.input(0).consume(n * 2048);
        sio.output(0).produce(n * 2048);

        Ok(())
    }
}

pub struct Keep1InN {
    alpha: f32,
    n: usize,
    i: usize,
    avg: [f32; 2048],
}

impl Keep1InN {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(alpha: f32, n: usize) -> Block {
        Block::new_sync(
            BlockMetaBuilder::new("Keep1InN").build(),
            StreamIoBuilder::new()
                .add_input("in", size_of::<f32>())
                .add_output("out", size_of::<f32>())
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                alpha,
                n,
                i: 0,
                avg: [0.0; 2048],
            },
        )
    }
}

#[async_trait]
impl SyncKernel for Keep1InN {
    fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let input = sio.input(0).slice::<f32>();
        let output = sio.output(0).slice::<f32>();

        let n = std::cmp::min(input.len(), output.len()) / 2048;

        for i in 0..n {
            for k in 0..2048 {
                let m = (k + 1024) % 2048;
                output[i * 2048 + m] = input[i * 2048 + k]
            }
        }

        if sio.input(0).finished() && n == input.len() / 2048 {
            io.finished = true;
        }

        sio.input(0).consume(n * 2048);
        sio.output(0).produce(n * 2048);

        Ok(())
    }
}




// use std::iter::repeat_with;
// use wasm_bindgen::prelude::*;

// use futuresdr::anyhow::Result;
// use futuresdr::blocks::CopyRandBuilder;
// use futuresdr::blocks::VectorSink;
// use futuresdr::blocks::VectorSinkBuilder;
// use futuresdr::blocks::VectorSourceBuilder;
// use futuresdr::log::info;
// use futuresdr::runtime::Flowgraph;
// use futuresdr::runtime::Runtime;

// #[wasm_bindgen]
// pub fn run_fg() {
//     run().unwrap();
// }

// fn run() -> Result<()> {
//     let mut fg = Flowgraph::new();

//     let n_items = 1_000;
//     let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(n_items).collect();

//     let src = VectorSourceBuilder::<f32>::new(orig.clone()).build();
//     let copy = CopyRandBuilder::new(4).max_copy(13).build();
//     let snk = VectorSinkBuilder::<f32>::new().build();

//     let src = fg.add_block(src);
//     let copy = fg.add_block(copy);
//     let snk = fg.add_block(snk);

//     fg.connect_stream(src, "out", copy, "in")?;
//     fg.connect_stream(copy, "out", snk, "in")?;

//     fg = Runtime::new().run(fg)?;

//     let snk = fg.block_async::<VectorSink<f32>>(snk).unwrap();
//     let v = snk.items();

//     assert_eq!(v.len(), n_items);
//     for i in 0..v.len() {
//         assert!((orig[i] - v[i]).abs() < f32::EPSILON);
//     }
//     info!("data matches");
//     info!("first items {:?}", &v[0..10]);

//     Ok(())
// }
