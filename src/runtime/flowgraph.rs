use async_lock::Mutex;
use async_lock::MutexGuard;
use std::fmt::Debug;
use std::sync::Arc;

use crate::runtime::Block;
use crate::runtime::BlockId;
use crate::runtime::BlockPortCtx;
use crate::runtime::BufferReader;
use crate::runtime::BufferWriter;
use crate::runtime::Error;
use crate::runtime::Kernel;
use crate::runtime::KernelInterface;
use crate::runtime::PortId;
use crate::runtime::WrappedKernel;

/// Reference to block that was added to the flowgraph.
pub struct BlockRef<K: Kernel> {
    id: BlockId,
    block: Arc<Mutex<WrappedKernel<K>>>,
}
impl<K: Kernel> BlockRef<K> {
    /// Get mutable, typed handle to [WrappedKernel].
    pub fn get(&self) -> MutexGuard<WrappedKernel<K>> {
        self.block.try_lock().unwrap()
    }
}
impl<K: Kernel> Clone for BlockRef<K> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            block: self.block.clone(),
        }
    }
}
impl<K: Kernel> Debug for BlockRef<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockRef")
            .field("id", &self.id)
            .field(
                "instance_name",
                &self.block.try_lock().unwrap().meta.instance_name(),
            )
            .finish()
    }
}
impl<K: Kernel> From<BlockRef<K>> for BlockId {
    fn from(value: BlockRef<K>) -> Self {
        value.id
    }
}
impl<K: Kernel> From<&BlockRef<K>> for BlockId {
    fn from(value: &BlockRef<K>) -> Self {
        value.id
    }
}

/// The main component of any FutureSDR program.
///
/// A [Flowgraph] is what drives the entire program. It is composed of a set of blocks and connections between them.
/// There is at least one source and one sink in every Flowgraph.
pub struct Flowgraph {
    pub(crate) blocks: Vec<Arc<Mutex<dyn Block>>>,
    pub(crate) stream_edges: Vec<(BlockId, PortId, BlockId, PortId)>,
    pub(crate) message_edges: Vec<(BlockId, PortId, BlockId, PortId)>,
}

impl Flowgraph {
    /// Creates a new [Flowgraph].
    pub fn new() -> Flowgraph {
        Flowgraph {
            blocks: Vec::new(),
            stream_edges: vec![],
            message_edges: vec![],
        }
    }

    /// Add [`Block`] to flowgraph
    pub fn add_block<K: Kernel + KernelInterface + 'static>(&mut self, block: K) -> BlockRef<K> {
        let block_id = BlockId(self.blocks.len());
        let mut b = WrappedKernel::new(block, block_id);
        let block_name = b.type_name();
        b.set_instance_name(&format!("{}-{}", block_name, block_id.0));
        let b = Arc::new(Mutex::new(b));
        self.blocks.push(b.clone());
        BlockRef {
            id: block_id,
            block: b,
        }
    }

    /// Make stream connection
    pub fn connect_stream<B: BufferWriter>(&mut self, src_port: &mut B, dst_port: &mut B::Reader) {
        self.stream_edges.push((
            src_port.block_id(),
            src_port.port_id(),
            dst_port.block_id(),
            dst_port.port_id(),
        ));
        src_port.connect(dst_port);
    }

    /// Connect stream ports non-type-safe
    pub fn connect_dyn(
        &mut self,
        src: impl Into<BlockId>,
        src_port: impl Into<PortId>,
        dst: impl Into<BlockId>,
        dst_port: impl Into<PortId>,
    ) -> Result<(), Error> {
        let src_id = src.into();
        let src_port = src_port.into();
        let dst = dst.into();
        let dst_port: PortId = dst_port.into();
        let src = self
            .blocks
            .get(src_id.0)
            .ok_or(Error::InvalidBlock(src_id))?;
        let dst = self.blocks.get(dst.0).ok_or(Error::InvalidBlock(dst))?;
        let mut tmp = dst.try_lock().unwrap();
        let reader = tmp
            .stream_input(dst_port.name())
            .ok_or(Error::InvalidStreamPort(BlockPortCtx::Id(src_id), dst_port))?;
        src.try_lock()
            .unwrap()
            .connect_stream_output(src_port.name(), reader)
    }

    /// Make message connection
    pub fn connect_message<K1: Kernel, K2: Kernel + KernelInterface>(
        &mut self,
        src_block: &BlockRef<K1>,
        src_port: impl Into<PortId>,
        dst_block: &BlockRef<K2>,
        dst_port: impl Into<PortId>,
    ) -> Result<(), Error> {
        let dst_box = dst_block.get().inbox_tx.clone();
        let src_port = src_port.into();
        let dst_port = dst_port.into();

        src_block.get().mio.connect(&src_port, dst_box, &dst_port)?;
        if !K2::message_inputs().to_owned().contains(&dst_port.name()) {
            return Err(Error::InvalidMessagePort(
                BlockPortCtx::Id(dst_block.id),
                dst_port,
            ));
        }
        self.message_edges
            .push((src_block.id, src_port, dst_block.id, dst_port));
        Ok(())
    }

    /// Get Block Dyn
    pub fn get(&self, id: BlockId) -> Arc<Mutex<dyn Block>> {
        self.blocks[id.0].clone()
    }

    /// Validate flowgraph
    ///
    /// Checks mainly that all stream ports are connected.
    pub fn validate(&self) -> Result<(), Error> {
        Ok(())
    }
}

impl Default for Flowgraph {
    fn default() -> Self {
        Self::new()
    }
}
