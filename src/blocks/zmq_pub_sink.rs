use anyhow::Result;

use crate::runtime::AsyncKernel;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

pub struct ZMQPubSink {
    item_size: usize,
    address: String,
    publisher: Option<zmq::Socket>,
}

impl ZMQPubSink {
    pub fn new(item_size: usize, address: &str) -> Block {
        Block::new_async(
            BlockMetaBuilder::new("ZMQPubSink").build(),
            StreamIoBuilder::new()
                .add_stream_input("in", item_size)
                .build(),
            MessageIoBuilder::new().build(),
            ZMQPubSink { item_size, address: address.to_string(), publisher: None },
        )
    }
}

#[async_trait]
impl AsyncKernel for ZMQPubSink {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<u8>();
        debug_assert_eq!(i.len() % self.item_size, 0);

        let n = i.len() / self.item_size;
        if n > 0 {
            print!(".");
            self.publisher.as_mut().unwrap().send(&*i, 0).unwrap();
            sio.input(0).consume(n);
        }

        if sio.input(0).finished() {
            io.finished = true;
        }

        Ok(())
    }

    async fn init(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let context = zmq::Context::new();
        let publisher = context.socket(zmq::PUB).unwrap();
        assert!(publisher.bind(&self.address).is_ok());
        self.publisher = Some(publisher.into());
        Ok(())
    }
}

pub struct ZMQPubSinkBuilder {
    item_size: usize,
    address: String,
}

impl ZMQPubSinkBuilder {
    pub fn new(item_size: usize) -> ZMQPubSinkBuilder {
        ZMQPubSinkBuilder {
            item_size,
            address: "tcp://*:5555".into(),
        }
    }

    pub fn address(mut self, address: &str) -> ZMQPubSinkBuilder {
        self.address = address.to_string();
        self
    }

    pub fn build(&mut self) -> Block {
        ZMQPubSink::new(self.item_size, &*self.address)
    }
}
