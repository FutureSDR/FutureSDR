use anyhow::Result;
use log::info;
use log::debug;

use crate::runtime::AsyncKernel;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

pub struct ZMQSubSource {
    item_size: usize,
    address: String,
    receiver: Option<zmq::Socket>,
}

impl ZMQSubSource {
    pub fn new(item_size: usize, address: &str) -> Block {
        Block::new_async(
            BlockMetaBuilder::new("ZMQSubSource").blocking().build(),
            StreamIoBuilder::new()
                .add_stream_output("out", item_size)
                .build(),
            MessageIoBuilder::new().build(),
            ZMQSubSource {
                item_size,
                address: address.to_string(),
                receiver: None,
            },
        )
    }
}

#[async_trait]
impl AsyncKernel for ZMQSubSource {
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let o = sio.output(0).slice::<u8>();
        if let Ok(n_bytes) = self.receiver.as_mut().unwrap().recv_into(o, 0) {
            debug!("ZMQ receiving\n");
            debug_assert_eq!(o.len() % self.item_size, 0);
            let n = n_bytes / self.item_size;
            sio.output(0).produce(n);
        }
        Ok(())
    }

    async fn init(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        debug!("ZMQSubSource Init\n");

        let context = zmq::Context::new();
        let receiver = context.socket(zmq::SUB).unwrap();
        info!("ZMQSubSource Connecting to {:?}", self.address);
        assert!(receiver.connect(&self.address).is_ok());
        receiver
            .set_subscribe(b"")
            .expect("cannot subscribe to ZMQ");
        self.receiver = Some(receiver.into());
        Ok(())
    }
}

pub struct ZMQSubSourceBuilder {
    item_size: usize,
    address: String,
}

impl ZMQSubSourceBuilder {
    pub fn new(item_size: usize) -> ZMQSubSourceBuilder {
        ZMQSubSourceBuilder {
            item_size,
            address: "tcp://*:5555".into(),
        }
    }

    pub fn address(mut self, address: &str) -> ZMQSubSourceBuilder {
        self.address = address.to_string();
        self
    }

    pub fn build(&mut self) -> Block {
        ZMQSubSource::new(self.item_size, &*self.address)
    }
}
