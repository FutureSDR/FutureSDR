use crate::runtime::BlockId;
use crate::runtime::BlockPortCtx;
use crate::runtime::Edge;
use crate::runtime::Error;
use crate::runtime::PortId;
use crate::runtime::Result;

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
        crate::runtime::block_on(self.message_async(
            src_block_id,
            src_port_id,
            dst_block_id,
            dst_port_id,
        ))
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

        let edge = Edge::new(src_block_id, src_port_id, dst_block_id, dst_port_id);
        self.validate_message_edge(&edge)?;
        self.message_edges.push(edge);
        Ok(())
    }

    fn validate_message_edge(&self, edge: &Edge) -> Result<(), Error> {
        let dst_inputs = self
            .blocks
            .get(edge.dst_block.0)
            .map(BlockSlot::message_inputs)
            .ok_or(Error::InvalidBlock(edge.dst_block))?;
        if !dst_inputs.contains(&edge.dst_port.name()) {
            return Err(Error::InvalidMessagePort(
                BlockPortCtx::Id(edge.dst_block),
                edge.dst_port.clone(),
            ));
        }

        let src_outputs = self
            .blocks
            .get(edge.src_block.0)
            .map(BlockSlot::message_outputs)
            .ok_or(Error::InvalidBlock(edge.src_block))?;
        if !src_outputs.contains(&edge.src_port.name()) {
            return Err(Error::InvalidMessagePort(
                BlockPortCtx::Id(edge.src_block),
                edge.src_port.clone(),
            ));
        }

        Ok(())
    }
}
