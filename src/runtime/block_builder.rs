use std::future::Future;
use std::pin::Pin;

use crate::anyhow::Result;
use crate::runtime::AsyncKernel;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Pmt;
use crate::runtime::StreamIoBuilder;
use crate::runtime::SyncKernel;

pub struct SyncNullKernel;
impl SyncKernel for SyncNullKernel {}

pub struct BlockBuilder;

impl BlockBuilder {
    pub fn new_async<K: AsyncKernel>(kernel: K) -> AsyncBlockBuilder<K> {
        AsyncBlockBuilder {
            kernel,
            meta: BlockMetaBuilder::new("MyBlock"),
            sio: StreamIoBuilder::new(),
            mio: MessageIoBuilder::new(),
        }
    }

    pub fn new_sync<K: SyncKernel>(kernel: K) -> SyncBlockBuilder<K> {
        SyncBlockBuilder {
            kernel,
            meta: BlockMetaBuilder::new("MyBlock"),
            sio: StreamIoBuilder::new(),
            mio: MessageIoBuilder::new(),
        }
    }

    pub fn new() -> SyncBlockBuilder<SyncNullKernel> {
        SyncBlockBuilder {
            kernel: SyncNullKernel,
            meta: BlockMetaBuilder::new("MyBlock"),
            sio: StreamIoBuilder::new(),
            mio: MessageIoBuilder::new(),
        }
    }
}

pub struct AsyncBlockBuilder<K: AsyncKernel + 'static> {
    kernel: K,
    meta: BlockMetaBuilder,
    sio: StreamIoBuilder,
    mio: MessageIoBuilder<K>,
}

impl<K: AsyncKernel> AsyncBlockBuilder<K> {
    pub fn name(mut self, name: &str) -> Self {
        self.meta = self.meta.name(name);
        self
    }

    pub fn blocking(mut self) -> Self {
        self.meta = self.meta.blocking();
        self
    }

    pub fn add_stream_input(mut self, name: &str, item_size: usize) -> Self {
        self.sio = self.sio.add_input(name, item_size);
        self
    }

    pub fn add_stream_output(mut self, name: &str, item_size: usize) -> Self {
        self.sio = self.sio.add_output(name, item_size);
        self
    }

    pub fn add_async_message_input(
        mut self,
        name: &str,
        c: impl for<'a> Fn(
                &'a mut K,
                &'a mut MessageIo<K>,
                &'a mut BlockMeta,
                Pmt,
            ) -> Pin<Box<dyn Future<Output = Result<Pmt>> + Send + 'a>>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.mio = self.mio.add_async_input(name, c);
        self
    }

    pub fn add_sync_message_input(
        mut self,
        name: &str,
        c: impl for<'a> Fn(&'a mut K, &'a mut MessageIo<K>, &'a mut BlockMeta, Pmt) -> Result<Pmt>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.mio = self.mio.add_sync_input(name, c);
        self
    }

    pub fn add_message_output(mut self, name: &str) -> Self {
        self.mio = self.mio.add_output(name);
        self
    }

    pub fn build(self) -> Block {
        Block::new_async(
            self.meta.build(),
            self.sio.build(),
            self.mio.build(),
            self.kernel,
        )
    }
}

pub struct SyncBlockBuilder<K: SyncKernel + 'static> {
    kernel: K,
    meta: BlockMetaBuilder,
    sio: StreamIoBuilder,
    mio: MessageIoBuilder<K>,
}

impl<K: SyncKernel> SyncBlockBuilder<K> {
    pub fn name(mut self, name: &str) -> Self {
        self.meta = self.meta.name(name);
        self
    }

    pub fn blocking(mut self) -> Self {
        self.meta = self.meta.blocking();
        self
    }

    pub fn add_stream_input(mut self, name: &str, item_size: usize) -> Self {
        self.sio = self.sio.add_input(name, item_size);
        self
    }

    pub fn add_stream_output(mut self, name: &str, item_size: usize) -> Self {
        self.sio = self.sio.add_output(name, item_size);
        self
    }

    pub fn add_async_message_input(
        mut self,
        name: &str,
        c: impl for<'a> Fn(
                &'a mut K,
                &'a mut MessageIo<K>,
                &'a mut BlockMeta,
                Pmt,
            ) -> Pin<Box<dyn Future<Output = Result<Pmt>> + Send + 'a>>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.mio = self.mio.add_async_input(name, c);
        self
    }

    pub fn add_sync_message_input(
        mut self,
        name: &str,
        c: impl for<'a> Fn(&'a mut K, &'a mut MessageIo<K>, &'a mut BlockMeta, Pmt) -> Result<Pmt>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.mio = self.mio.add_sync_input(name, c);
        self
    }

    pub fn add_message_output(mut self, name: &str) -> Self {
        self.mio = self.mio.add_output(name);
        self
    }

    pub fn build(self) -> Block {
        Block::new_sync(
            self.meta.build(),
            self.sio.build(),
            self.mio.build(),
            self.kernel,
        )
    }
}
