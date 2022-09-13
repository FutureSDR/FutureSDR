#[cfg(not(target_arch = "wasm32"))]
use axum::Router;
use futures::channel::mpsc::Sender;
use futures::channel::oneshot;
use futures::SinkExt;
use futuresdr_pmt::BlockDescription;
use futuresdr_pmt::FlowgraphDescription;
use std::cmp::{Eq, PartialEq};
use std::fmt::Debug;
use std::hash::Hash;

use crate::anyhow::Result;
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::buffer::circular::Circular;
#[cfg(target_arch = "wasm32")]
use crate::runtime::buffer::slab::Slab;
use crate::runtime::buffer::BufferBuilder;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::Block;
use crate::runtime::BlockMessage;
use crate::runtime::FlowgraphMessage;
use crate::runtime::Kernel;
use crate::runtime::Pmt;
use crate::runtime::Topology;

/// The main component of any FutureSDR program.
///
/// A [Flowgraph] is what drives the entire program. It is composed of a set of blocks and connections between them.
/// There is at least one source and one sink in every Flowgraph.
pub struct Flowgraph {
    pub(crate) topology: Option<Topology>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) custom_routes: Option<Router>,
}

impl Flowgraph {
    /// Creates a new [Flowgraph] with an empty [Topology]
    pub fn new() -> Flowgraph {
        Flowgraph {
            topology: Some(Topology::new()),
            #[cfg(not(target_arch = "wasm32"))]
            custom_routes: None,
        }
    }

    pub fn add_block(&mut self, block: Block) -> usize {
        self.topology.as_mut().unwrap().add_block(block)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_custom_routes(&mut self, routes: Router) {
        self.custom_routes = Some(routes);
    }

    pub fn connect_stream(
        &mut self,
        src_block: usize,
        src_port: &str,
        dst_block: usize,
        dst_port: &str,
    ) -> Result<()> {
        self.topology.as_mut().unwrap().connect_stream(
            src_block,
            src_port,
            dst_block,
            dst_port,
            DefaultBuffer::new(),
        )
    }

    pub fn connect_stream_with_type<B: BufferBuilder + Debug + Eq + Hash>(
        &mut self,
        src_block: usize,
        src_port: &str,
        dst_block: usize,
        dst_port: &str,
        buffer: B,
    ) -> Result<()> {
        self.topology
            .as_mut()
            .unwrap()
            .connect_stream(src_block, src_port, dst_block, dst_port, buffer)
    }

    pub fn connect_message(
        &mut self,
        src_block: usize,
        src_port: &str,
        dst_block: usize,
        dst_port: &str,
    ) -> Result<()> {
        self.topology
            .as_mut()
            .unwrap()
            .connect_message(src_block, src_port, dst_block, dst_port)
    }

    pub fn kernel<T: Kernel + 'static>(&self, id: usize) -> Option<&T> {
        self.topology
            .as_ref()
            .and_then(|t| t.block_ref(id))
            .and_then(|b| b.kernel())
    }

    pub fn kernel_mut<T: Kernel + 'static>(&mut self, id: usize) -> Option<&T> {
        self.topology
            .as_mut()
            .and_then(|t| t.block_mut(id))
            .and_then(|b| b.kernel_mut())
    }
}

impl Default for Flowgraph {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct FlowgraphHandle {
    inbox: Sender<FlowgraphMessage>,
}

impl FlowgraphHandle {
    pub(crate) fn new(inbox: Sender<FlowgraphMessage>) -> FlowgraphHandle {
        FlowgraphHandle { inbox }
    }

    pub async fn call(&mut self, block_id: usize, port_id: usize, data: Pmt) -> Result<()> {
        self.inbox
            .send(FlowgraphMessage::BlockCall {
                block_id,
                port_id,
                data,
            })
            .await?;
        Ok(())
    }

    pub async fn callback(&mut self, block_id: usize, port_id: usize, data: Pmt) -> Result<Pmt> {
        let (tx, rx) = oneshot::channel::<Pmt>();
        self.inbox
            .send(FlowgraphMessage::BlockCallback {
                block_id,
                port_id,
                data,
                tx,
            })
            .await?;
        let p = rx.await?;
        Ok(p)
    }

    pub async fn description(&mut self) -> Result<FlowgraphDescription> {
        let (tx, rx) = oneshot::channel::<FlowgraphDescription>();
        self.inbox
            .send(FlowgraphMessage::FlowgraphDescription { tx })
            .await?;
        let d = rx.await?;
        Ok(d)
    }

    pub async fn block_description(&mut self, block_id: usize) -> Result<BlockDescription> {
        let (tx, rx) = oneshot::channel::<BlockDescription>();
        self.inbox
            .send(FlowgraphMessage::BlockDescription { block_id, tx })
            .await?;
        let d = rx.await?;
        Ok(d)
    }

    pub async fn terminate(&mut self) -> Result<()> {
        self.inbox.send(FlowgraphMessage::Terminate).await?;
        Ok(())
    }
}

#[derive(Debug, PartialEq, Hash)]
pub struct DefaultBuffer;

impl Eq for DefaultBuffer {}

impl DefaultBuffer {
    fn new() -> DefaultBuffer {
        DefaultBuffer
    }
}

impl BufferBuilder for DefaultBuffer {
    #[cfg(not(target_arch = "wasm32"))]
    fn build(
        &self,
        item_size: usize,
        writer_inbox: Sender<BlockMessage>,
        writer_output_id: usize,
    ) -> BufferWriter {
        Circular::new().build(item_size, writer_inbox, writer_output_id)
    }
    #[cfg(target_arch = "wasm32")]
    fn build(
        &self,
        item_size: usize,
        writer_inbox: Sender<BlockMessage>,
        writer_output_id: usize,
    ) -> BufferWriter {
        Slab::new().build(item_size, writer_inbox, writer_output_id)
    }
}
