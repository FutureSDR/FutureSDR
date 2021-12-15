use futures::channel::mpsc;
use futures::SinkExt;
use futuresdr_frontend::gui::frequency;
use std::mem::size_of;

use crate::anyhow::Result;
use crate::runtime::AsyncKernel;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

pub struct WasmFreq {
    sender: mpsc::Sender<Vec<f32>>,
}

impl WasmFreq {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(div: &str, min: f32, max: f32) -> Block {

        let sender = frequency::mount(div, min, max);

        Block::new_async(
            BlockMetaBuilder::new("WasmFreq").build(),
            StreamIoBuilder::new()
                .add_input("in", size_of::<f32>())
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                sender,
            },
        )
    }
}

#[async_trait]
impl AsyncKernel for WasmFreq {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {

        let input = sio.input(0).slice::<f32>();
        let n = input.len() / 2048;

        for i in 0..n {
            if self.sender.send(input[i*2048..(i+1)*2048].to_vec()).await.is_err() {
                info!("WasmFreq failed to write to Yew component. Terminating");
                io.finished = true;
            }
            info!("WasmFreq sent data");
        }

        sio.input(0).consume(n * 2048);

        Ok(())
    }
}
