use futures::channel::mpsc;
use futures::SinkExt;
use futures::StreamExt;
use once_cell::sync::OnceCell;
use std::mem::size_of;
use std::sync::Mutex;
use wasm_bindgen::prelude::*;

use crate::anyhow::Result;
use crate::num_complex::Complex32;
use crate::runtime::AsyncKernel;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

static SENDER: OnceCell<Mutex<mpsc::Sender<Vec<u8>>>> = OnceCell::new();

#[wasm_bindgen]
pub async fn push_samples(s: Vec<u8>) -> bool {
    if let Some(tx) = SENDER.get() {
        if tx.lock().unwrap().send(s).await.is_err() {
            info!("WasmSdr, pushing while closed");
            false
        } else {
            true
        }
    } else {
        info!("WasmSdr, pushing before initialized");
        false
    }
}

pub struct WasmSdr {
    receiver: mpsc::Receiver<Vec<u8>>,
    samples: Vec<u8>,
    index: usize,
}

impl WasmSdr {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Block {
        let (sender, receiver) = mpsc::channel(1);
        SENDER.set(Mutex::new(sender)).unwrap();

        Block::new_async(
            BlockMetaBuilder::new("WasmSDR").build(),
            StreamIoBuilder::new()
                .add_output("out", size_of::<Complex32>())
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                receiver,
                samples: Vec::new(),
                index: 0,
            },
        )
    }
}

#[async_trait]
impl AsyncKernel for WasmSdr {
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let output = sio.output(0).slice::<Complex32>();

        if self.index == self.samples.len() {
            self.samples = self.receiver.next().await.unwrap();
            self.index = 0;
        }

        let n = std::cmp::min((self.samples.len() - self.index) / 2, output.len());

        for i in 0..n {
            output[i] = Complex32::new(
                (self.samples[i * 2] as f32 - 128.0) / 128.0,
                (self.samples[i * 2 + 1] as f32 - 128.0) / 128.0,
            );
        }

        self.index += 2 * n;
        sio.output(0).produce(n);

        Ok(())
    }
}
