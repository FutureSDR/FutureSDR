use futures::channel::mpsc;
use futures::SinkExt;
use futures::StreamExt;
use once_cell::sync::OnceCell;
use std::mem::size_of;
use std::sync::Mutex;
use wasm_bindgen::prelude::*;

use crate::anyhow::Result;
use crate::num_complex::Complex32;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

static SENDER: OnceCell<Mutex<mpsc::Sender<Vec<i8>>>> = OnceCell::new();

// there should be no one else contenting for this lock
// to make sure, we use try_lock().unwrap(), which would panic
// if the lock is held by someone else
#[allow(clippy::await_holding_lock)]
#[wasm_bindgen]
pub async fn push_samples(s: Vec<i8>) -> bool {
    if let Some(tx) = SENDER.get() {
        if tx.try_lock().unwrap().send(s).await.is_err() {
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
    receiver: mpsc::Receiver<Vec<i8>>,
    samples: Vec<i8>,
    index: usize,
}

impl WasmSdr {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Block {
        let (sender, receiver) = mpsc::channel(1);
        SENDER.set(Mutex::new(sender)).unwrap();

        Block::new(
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

#[doc(hidden)]
#[async_trait]
impl Kernel for WasmSdr {
    async fn work(
        &mut self,
        io: &mut WorkIo,
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

        for (i, o) in output.iter_mut().enumerate().take(n) {
            *o = Complex32::new(
                (self.samples[self.index + i * 2] as f32) / 128.0,
                (self.samples[self.index + i * 2 + 1] as f32) / 128.0,
            );
        }

        self.index += 2 * n;

        sio.output(0).produce(n);
        if n < output.len() {
            io.call_again = true;
        }

        Ok(())
    }
}
