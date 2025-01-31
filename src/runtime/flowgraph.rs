use slab::Slab;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;

use crate::runtime::Block;
use crate::runtime::BlockId;
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
    fn get(&self) -> MutexGuard<WrappedKernel<K>> {
        self.block.lock().unwrap()
    }
}

/// The main component of any FutureSDR program.
///
/// A [Flowgraph] is what drives the entire program. It is composed of a set of blocks and connections between them.
/// There is at least one source and one sink in every Flowgraph.
pub struct Flowgraph {
    blocks: Slab<Arc<Mutex<dyn Block>>>,
    stream_edges: Vec<(BlockId, PortId, BlockId, PortId)>,
    message_edges: Vec<(BlockId, PortId, BlockId, PortId)>,
}

impl Flowgraph {
    /// Creates a new [Flowgraph].
    pub fn new() -> Flowgraph {
        Flowgraph {
            blocks: Slab::new(),
            stream_edges: vec![],
            message_edges: vec![],
        }
    }

    /// Add [`Block`] to flowgraph
    pub fn add_block<K: Kernel + KernelInterface>(&mut self, block: K) -> BlockRef<K> {
        let b = WrappedKernel::new(block);
        let block_name = b.type_name();
        let block_id = self.blocks.vacant_key();
        b.set_instance_name(format!("{}-{}", block_name, block_id));
        let b = Arc::new(Mutex::new(b));
        self.blocks.insert(b.clone());
        BlockRef {
            id: block_id.into(),
            block: b,
        }
    }

    fn connect_stream<B: BufferWriter>(&mut self, src_port: &mut B, dst_port: &mut B::Reader) {
        self.stream_edges.push((
            src_port.block_id(),
            src_port.port_id(),
            dst_port.block_id(),
            dst_port.port_id(),
        ));
        src_port.connect(dst_port);
    }
    /// Make stream connection
    pub fn connect_stream(
        &mut self,
        src_block: BlockId,
        src_port: impl Into<PortId>,
        dst_block: BlockId,
        dst_port: impl Into<PortId>,
    ) {
        self.stream_edges
            .push((src_block, src_port.into(), dst_block, dst_port.into()));
    }

    /// Make message connection
    pub fn connect_message(
        &mut self,
        src_block: usize,
        src_port: impl Into<PortId>,
        dst_block: usize,
        dst_port: impl Into<PortId>,
    ) {
        self.message_edges
            .push((src_block, src_port.into(), dst_block, dst_port.into()));
    }

    pub fn validate(&self) -> Result<(), Error> {
        Ok(())
    }
}

impl Default for Flowgraph {
    fn default() -> Self {
        Self::new()
    }
}
