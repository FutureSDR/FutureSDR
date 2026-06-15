use crate::runtime::BlockId;
use crate::runtime::BlockPortCtx;
use crate::runtime::Edge;
use crate::runtime::Error;
use crate::runtime::PortId;
use crate::runtime::Result;
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::block_on;
use crate::runtime::resolve_port_name;

use super::BlockSlot;
use super::Flowgraph;

impl Flowgraph {
    /// Connect a message output port to a message input port.
    ///
    /// Message connections are type-erased and may form arbitrary topologies,
    /// including cycles and self-connections. The destination message input is
    /// and the source message output are validated immediately. The concrete
    /// output handler list is populated from this logical edge at startup.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn message(
        &mut self,
        src_block_id: impl Into<BlockId>,
        src_port_id: impl Into<PortId>,
        dst_block_id: impl Into<BlockId>,
        dst_port_id: impl Into<PortId>,
    ) -> Result<(), Error> {
        block_on(self.message_async(src_block_id, src_port_id, dst_block_id, dst_port_id))
    }

    /// Async counterpart to [`Flowgraph::message`].
    pub async fn message_async(
        &mut self,
        src_block_id: impl Into<BlockId>,
        src_port_id: impl Into<PortId>,
        dst_block_id: impl Into<BlockId>,
        dst_port_id: impl Into<PortId>,
    ) -> Result<(), Error> {
        let src_block_id = src_block_id.into();
        let src_port_id = src_port_id.into();
        let dst_block_id = dst_block_id.into();
        let dst_port_id = dst_port_id.into();

        let edge =
            self.normalize_message_edge(src_block_id, src_port_id, dst_block_id, dst_port_id)?;
        self.message_edges.push(edge);
        Ok(())
    }

    fn normalize_message_port(
        block_id: BlockId,
        port_id: PortId,
        ports: &[&str],
    ) -> Result<PortId, Error> {
        resolve_port_name(&port_id, ports).ok_or(Error::InvalidMessagePort(
            BlockPortCtx::Id(block_id),
            port_id,
        ))
    }

    fn normalize_message_edge(
        &self,
        src_block: BlockId,
        src_port: PortId,
        dst_block: BlockId,
        dst_port: PortId,
    ) -> Result<Edge, Error> {
        let dst_inputs = self
            .blocks
            .get(dst_block.0)
            .map(BlockSlot::message_inputs)
            .ok_or(Error::InvalidBlock(dst_block))?;
        let dst_port = Self::normalize_message_port(dst_block, dst_port, dst_inputs)?;

        let src_outputs = self
            .blocks
            .get(src_block.0)
            .map(BlockSlot::message_outputs)
            .ok_or(Error::InvalidBlock(src_block))?;
        let src_port = Self::normalize_message_port(src_block, src_port, src_outputs)?;

        Ok(Edge::new(src_block, src_port, dst_block, dst_port))
    }
}
