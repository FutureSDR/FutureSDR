use wasm_bindgen::prelude::*;

use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

#[wasm_bindgen]
extern "C" {
    fn put_samples(s: Vec<f32>);
}

pub struct WasmFreq;

impl WasmFreq {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("WasmFreq").build(),
            StreamIoBuilder::new().add_input::<f32>("in").build(),
            MessageIoBuilder::new().build(),
            Self,
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for WasmFreq {
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let input = sio.input(0).slice::<f32>();
        let n = input.len() / 2048;

        for i in 0..n {
            put_samples(input[i * 2048..(i + 1) * 2048].to_vec());
        }

        sio.input(0).consume(n * 2048);

        Ok(())
    }
}
