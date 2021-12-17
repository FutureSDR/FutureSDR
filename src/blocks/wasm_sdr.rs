use std::mem::size_of;
use wasm_bindgen::prelude::*;

use crate::anyhow::Result;
use crate::num_complex::Complex32;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::SyncKernel;
use crate::runtime::WorkIo;

#[wasm_bindgen]
extern "C" {
    fn read_samples() -> Vec<u8>;
}

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

        if self.index == self.samples.len() {
            self.samples = read_samples();
            self.index = 0;
        }

        let n = std::cmp::min((self.samples.len() - self.index) / 2, output.len());

        for i in 0..n {
            output[i] = Complex32::new(
                (self.samples[i * 2    ] as f32 - 128.0) / 128.0,
                (self.samples[i * 2 + 1] as f32 - 128.0) / 128.0);
        }

        self.index += 2 * n;
        sio.output(0).produce(n);

        Ok(())
    }
}
