use futuresdr::anyhow::Result;
use futuresdr::blocks::Fft;
use futuresdr::blocks::NullSink;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use wasm_bindgen::prelude::*;

use crate::lin2db_block;
use crate::power_block;
use crate::FftShift;
use crate::Keep1InN;

#[wasm_bindgen]
pub fn run_fg() {
    run().unwrap();
}

fn run() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = fg.add_block(WasmSdr::new());
    let fft = fg.add_block(Fft::new());
    let power = fg.add_block(power_block());
    let log = fg.add_block(lin2db_block());
    let shift = fg.add_block(FftShift::<f32>::new());
    let keep = fg.add_block(Keep1InN::new(0.1, 10));
    let snk = fg.add_block(NullSink::new(4));

    fg.connect_stream(src, "out", fft, "in")?;
    fg.connect_stream(fft, "out", power, "in")?;
    fg.connect_stream(power, "out", log, "in")?;
    fg.connect_stream(log, "out", shift, "in")?;
    fg.connect_stream(shift, "out", keep, "in")?;
    fg.connect_stream(keep, "out", snk, "in")?;

    Runtime::new().run(fg)?;
    Ok(())
}

#[wasm_bindgen]
extern "C" {
    fn read_samples() -> Vec<u8>;
    fn set_freq(f: u32);
}

use std::mem::size_of;

use futuresdr::async_trait::async_trait;
use futuresdr::log::info;
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

pub struct WasmSdr {
    samples: Vec<u8>,
    index: usize,
}

impl WasmSdr {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Block {
        Block::new_sync(
            BlockMetaBuilder::new("WasmSDR").build(),
            StreamIoBuilder::new()
                .add_output("out", size_of::<Complex32>())
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                samples: Vec::new(),
                index: 0,
            },
        )
    }
}

#[async_trait]
impl SyncKernel for WasmSdr {
    fn work(
        &mut self,
        _io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {

        let output = sio.output(0).slice::<Complex32>();

        while self.index == self.samples.len() {
            self.samples = read_samples();
            self.index = 0;
        }

        let n = std::cmp::min((self.samples.len() - self.index) / 2, output.len());

        for i in 0..n {
            output[i] = Complex32::new(
                (self.samples[i * 2    ] as f32 - 128.0) / 128.0,
                (self.samples[i * 2 + 1] as f32 - 128.0) / 128.0)
        }

        info!("producing {}", n);
        self.index += 2 * n;
        sio.output(0).produce(n);

        Ok(())
    }
}
