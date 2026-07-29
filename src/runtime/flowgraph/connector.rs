use std::any::Any;
use std::any::TypeId;
use std::sync::Arc;

use crate::runtime::BlockId;
use crate::runtime::Edge;
use crate::runtime::Error;
use crate::runtime::PortId;
use crate::runtime::PortIndex;
use crate::runtime::Result;
use crate::runtime::block::BlockObject;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferRequirements;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::DynThreadSafeConnect;
use crate::runtime::buffer::DynThreadSafeToken;
use crate::runtime::buffer::ThreadSafeConnect;

use super::Flowgraph;
use super::types::BlockLocation;

#[derive(Debug, Copy, Clone)]
pub(super) struct ResolvedEdge {
    pub(super) src_block: BlockId,
    pub(super) src_port: PortIndex,
    pub(super) dst_block: BlockId,
    pub(super) dst_port: PortIndex,
}

impl ResolvedEdge {
    pub(super) fn from_indexed(edge: Edge) -> Self {
        Self {
            src_block: edge.src_block,
            src_port: edge.src_port.index_value(),
            dst_block: edge.dst_block,
            dst_port: edge.dst_port.index_value(),
        }
    }
}

#[derive(Debug)]
struct ResolvedStreamGroup {
    src_block: BlockId,
    src_port: PortIndex,
    dsts: Vec<(BlockId, PortIndex)>,
}

impl ResolvedStreamGroup {
    fn new(src_block: BlockId, src_port: PortIndex) -> Self {
        Self {
            src_block,
            src_port,
            dsts: Vec::new(),
        }
    }

    fn push(&mut self, dst_block: BlockId, dst_port: PortIndex) {
        self.dsts.push((dst_block, dst_port));
    }
}

struct StreamOutputInfo {
    reader_type_id: TypeId,
    max_readers: usize,
    requirements: BufferRequirements,
    thread_safe_connect: Option<Arc<dyn DynThreadSafeConnect>>,
}

struct StreamInputInfo {
    concrete_type_id: TypeId,
    requirements: BufferRequirements,
}

struct ValidatedStreamGroup {
    src_requirements: BufferRequirements,
    dst_requirements: Vec<BufferRequirements>,
    thread_safe_connect: Option<Arc<dyn DynThreadSafeConnect>>,
}

pub(super) struct FlowgraphConnector<'a> {
    flowgraph: &'a mut Flowgraph,
}

impl<'a> FlowgraphConnector<'a> {
    pub(super) fn new(flowgraph: &'a mut Flowgraph) -> Self {
        Self { flowgraph }
    }

    #[allow(clippy::too_many_arguments)]
    fn connect_stream_ports_dyn(
        src_block_id: BlockId,
        src_port: PortIndex,
        src_block: &mut dyn BlockObject,
        src_requirements: BufferRequirements,
        dst_block_id: BlockId,
        dst_port: PortIndex,
        dst_block: &mut dyn BlockObject,
        dst_requirements: BufferRequirements,
    ) -> Result<(), Error> {
        let (_, reader) = dst_block
            .stream_input_at(dst_port)
            .ok_or_else(|| Error::InvalidStreamPort(dst_block_id, PortId::from(dst_port)))?;
        reader.raise_buffer_requirements(dst_requirements);

        let (_, writer) = src_block
            .stream_output_at(src_port)
            .ok_or_else(|| Error::InvalidStreamPort(src_block_id, PortId::from(src_port)))?;
        writer.raise_buffer_requirements(src_requirements);

        writer.connect_dyn(reader)?;

        Ok(())
    }

    async fn typed_stream_output_port_id_async<KS, B, FS>(
        &mut self,
        location: BlockLocation,
        src_port: FS,
    ) -> Result<PortId, Error>
    where
        KS: 'static,
        B: BufferWriter,
        FS: FnOnce(&mut KS) -> &mut B + Send + 'static,
    {
        self.flowgraph
            .with_typed_kernel_mut::<KS, _>(location, move |kernel| {
                Ok(PortId::from(src_port(kernel).port_id()))
            })
            .await
    }

    async fn typed_stream_input_port_id_async<KD, B, FD>(
        &mut self,
        location: BlockLocation,
        dst_port: FD,
    ) -> Result<PortId, Error>
    where
        KD: 'static,
        B: BufferWriter,
        FD: FnOnce(&mut KD) -> &mut B::Reader + Send + 'static,
    {
        self.flowgraph
            .with_typed_kernel_mut::<KD, _>(location, move |kernel| {
                Ok(PortId::from(dst_port(kernel).port_id()))
            })
            .await
    }

    pub(super) async fn cross_domain_stream_edge_async<KS, KD, B, FS, FD>(
        &mut self,
        src: BlockLocation,
        src_port: FS,
        dst: BlockLocation,
        dst_port: FD,
    ) -> Result<Edge, Error>
    where
        KS: 'static,
        KD: 'static,
        B: ThreadSafeConnect,
        FS: FnOnce(&mut KS) -> &mut B + Send + 'static,
        FD: FnOnce(&mut KD) -> &mut B::Reader + Send + 'static,
    {
        let src_port_id = self
            .typed_stream_output_port_id_async::<KS, B, FS>(src, src_port)
            .await?;
        let dst_port_id = self
            .typed_stream_input_port_id_async::<KD, B, FD>(dst, dst_port)
            .await?;
        Ok(Edge::new(
            src.block_id,
            src_port_id,
            dst.block_id,
            dst_port_id,
        ))
    }

    async fn connect_same_domain_stream_dyn_async(
        &mut self,
        src: BlockLocation,
        src_port: PortIndex,
        src_requirements: BufferRequirements,
        dst: BlockLocation,
        dst_port: PortIndex,
        dst_requirements: BufferRequirements,
    ) -> Result<(), Error> {
        let src_block_id = src.block_id;
        let dst_block_id = dst.block_id;
        self.flowgraph
            .with_same_domain_two_blocks_mut(src, dst, move |src_block, dst_block| {
                Self::connect_stream_ports_dyn(
                    src_block_id,
                    src_port,
                    src_block,
                    src_requirements,
                    dst_block_id,
                    dst_port,
                    dst_block,
                    dst_requirements,
                )
            })
            .await
    }

    async fn take_stream_input_connect_token(
        &mut self,
        location: BlockLocation,
        port: PortIndex,
        requirements: BufferRequirements,
        connect: Arc<dyn DynThreadSafeConnect>,
    ) -> Result<DynThreadSafeToken, Error> {
        self.flowgraph
            .with_block_mut(location, move |block| {
                let (_, reader) = block.stream_input_at(port).ok_or_else(|| {
                    Error::InvalidStreamPort(location.block_id, PortId::from(port))
                })?;
                reader.raise_buffer_requirements(requirements);
                connect.take_reader(reader)
            })
            .await
    }

    async fn connect_stream_output_token(
        &mut self,
        location: BlockLocation,
        port: PortIndex,
        requirements: BufferRequirements,
        token: DynThreadSafeToken,
        connect: Arc<dyn DynThreadSafeConnect>,
    ) -> Result<DynThreadSafeToken, Error> {
        self.flowgraph
            .with_block_mut(location, move |block| {
                let (_, writer) = block.stream_output_at(port).ok_or_else(|| {
                    Error::InvalidStreamPort(location.block_id, PortId::from(port))
                })?;
                writer.raise_buffer_requirements(requirements);
                connect.connect_reader(writer, token)
            })
            .await
    }

    async fn finish_stream_input_connect(
        &mut self,
        token: DynThreadSafeToken,
        location: BlockLocation,
        port: PortIndex,
        connect: Arc<dyn DynThreadSafeConnect>,
    ) -> Result<(), Error> {
        self.flowgraph
            .with_block_mut(location, move |block| {
                let (_, reader) = block.stream_input_at(port).ok_or_else(|| {
                    Error::InvalidStreamPort(location.block_id, PortId::from(port))
                })?;
                connect.finish_reader(reader, token)
            })
            .await
    }

    async fn connect_cross_domain_stream_dyn_async(
        &mut self,
        src: BlockLocation,
        src_port: PortIndex,
        src_requirements: BufferRequirements,
        dsts: &[(BlockLocation, PortIndex, BufferRequirements)],
        connect: Arc<dyn DynThreadSafeConnect>,
    ) -> Result<(), Error> {
        for (dst, dst_port, dst_requirements) in dsts {
            let token = self
                .take_stream_input_connect_token(
                    *dst,
                    *dst_port,
                    *dst_requirements,
                    connect.clone(),
                )
                .await?;
            let return_token = self
                .connect_stream_output_token(
                    src,
                    src_port,
                    src_requirements,
                    token,
                    connect.clone(),
                )
                .await?;
            self.finish_stream_input_connect(return_token, *dst, *dst_port, connect.clone())
                .await?;
        }
        Ok(())
    }

    fn stream_groups(edges: &[ResolvedEdge]) -> Vec<ResolvedStreamGroup> {
        let mut groups = Vec::<ResolvedStreamGroup>::new();
        for edge in edges {
            if let Some(group) = groups
                .iter_mut()
                .find(|group| group.src_block == edge.src_block && group.src_port == edge.src_port)
            {
                group.push(edge.dst_block, edge.dst_port);
            } else {
                let mut group = ResolvedStreamGroup::new(edge.src_block, edge.src_port);
                group.push(edge.dst_block, edge.dst_port);
                groups.push(group);
            }
        }
        groups
    }

    async fn stream_output_info(
        &mut self,
        location: BlockLocation,
        port: PortIndex,
    ) -> Result<StreamOutputInfo, Error> {
        self.flowgraph
            .with_block_mut(location, move |block| {
                let (_, writer) = block.stream_output_at(port).ok_or_else(|| {
                    Error::InvalidStreamPort(location.block_id, PortId::from(port))
                })?;
                Ok(StreamOutputInfo {
                    reader_type_id: writer.reader_type_id(),
                    max_readers: writer.max_readers(),
                    requirements: writer.buffer_requirements(),
                    thread_safe_connect: writer.thread_safe_connect(),
                })
            })
            .await
    }

    async fn stream_input_info(
        &mut self,
        location: BlockLocation,
        port: PortIndex,
    ) -> Result<StreamInputInfo, Error> {
        self.flowgraph
            .with_block_mut(location, move |block| {
                let (_, reader) = block.stream_input_at(port).ok_or_else(|| {
                    Error::InvalidStreamPort(location.block_id, PortId::from(port))
                })?;
                Ok(StreamInputInfo {
                    concrete_type_id: (&*reader as &dyn Any).type_id(),
                    requirements: reader.buffer_requirements(),
                })
            })
            .await
    }

    async fn validate_stream_group(
        &mut self,
        group: &ResolvedStreamGroup,
    ) -> Result<ValidatedStreamGroup, Error> {
        let src_name = self
            .flowgraph
            .stream_output_name(group.src_block, &PortId::from(group.src_port))?
            .name()
            .to_string();
        let src_location = self.flowgraph.location(group.src_block)?;
        let src = self
            .stream_output_info(src_location, group.src_port)
            .await?;
        if group.dsts.len() > src.max_readers {
            return Err(Error::ValidationError(format!(
                "stream output {:?}.{} supports at most {} reader(s)",
                group.src_block, src_name, src.max_readers
            )));
        }

        let mut merged = src.requirements;
        let mut dst_own_requirements = Vec::with_capacity(group.dsts.len());
        for (dst_block, dst_port) in &group.dsts {
            let dst_location = self.flowgraph.location(*dst_block)?;
            let dst = self.stream_input_info(dst_location, *dst_port).await?;
            if src.reader_type_id != dst.concrete_type_id {
                return Err(Error::ValidationError(
                    "dyn BufferReader has wrong type".to_string(),
                ));
            }
            merged.merge(dst.requirements);
            dst_own_requirements.push(dst.requirements);
        }

        let src_requirements = Self::with_merged_buffer_size(src.requirements, merged);
        let dst_requirements = dst_own_requirements
            .iter()
            .copied()
            .map(|requirements| Self::with_merged_buffer_size(requirements, merged))
            .collect();
        Ok(ValidatedStreamGroup {
            src_requirements,
            dst_requirements,
            thread_safe_connect: src.thread_safe_connect,
        })
    }

    fn with_merged_buffer_size(
        mut requirements: BufferRequirements,
        merged: BufferRequirements,
    ) -> BufferRequirements {
        if let Some(min_items) = merged.min_buffer_size_in_items() {
            requirements.raise_min_buffer_size_in_items(min_items);
        }
        requirements
    }

    async fn apply_stream_group(&mut self, group: &ResolvedStreamGroup) -> Result<(), Error> {
        let ValidatedStreamGroup {
            src_requirements,
            dst_requirements,
            thread_safe_connect,
        } = self.validate_stream_group(group).await?;
        let src = self.flowgraph.location(group.src_block)?;
        let dsts = group
            .dsts
            .iter()
            .zip(dst_requirements)
            .map(|((dst_block, dst_port), requirements)| {
                Ok((
                    self.flowgraph.location(*dst_block)?,
                    *dst_port,
                    requirements,
                ))
            })
            .collect::<Result<Vec<_>, Error>>()?;

        // Fanout-capable buffers allocate the shared backing storage on the
        // first connect. Connect the most demanding readers first without
        // turning inferred min-items into an explicit buffer-size setting.
        let mut dsts = dsts;
        dsts.sort_by(|(_, _, a), (_, _, b)| {
            b.min_items().unwrap_or(1).cmp(&a.min_items().unwrap_or(1))
        });

        if dsts
            .iter()
            .all(|(dst, _, _)| dst.domain_id == src.domain_id)
        {
            for (dst, dst_port, dst_requirements) in dsts {
                self.connect_same_domain_stream_dyn_async(
                    src,
                    group.src_port,
                    src_requirements,
                    dst,
                    dst_port,
                    dst_requirements,
                )
                .await?;
            }
            Ok(())
        } else {
            let connect = thread_safe_connect.ok_or_else(|| {
                Error::ValidationError(
                    "stream buffer does not provide thread-safe connection tokens".to_string(),
                )
            })?;
            self.connect_cross_domain_stream_dyn_async(
                src,
                group.src_port,
                src_requirements,
                &dsts,
                connect,
            )
            .await
        }
    }

    pub(super) async fn apply_stream_edges(&mut self, edges: &[ResolvedEdge]) -> Result<(), Error> {
        for group in Self::stream_groups(edges) {
            self.apply_stream_group(&group).await?;
        }
        Ok(())
    }

    async fn apply_message_edge(&mut self, edge: ResolvedEdge) -> Result<(), Error> {
        let ResolvedEdge {
            src_block,
            src_port,
            dst_block,
            dst_port,
        } = edge;
        let src = self.flowgraph.location(src_block)?;
        let dst = self
            .flowgraph
            .blocks
            .get(dst_block.0)
            .ok_or(Error::InvalidBlock(dst_block))?
            .endpoint()
            .clone();

        self.flowgraph
            .with_block_mut(src, move |src_block| {
                src_block.connect_message(src_port, dst, dst_port)
            })
            .await
    }

    pub(super) async fn apply_message_edges(
        &mut self,
        edges: &[ResolvedEdge],
    ) -> Result<(), Error> {
        for edge in edges.iter().cloned() {
            self.apply_message_edge(edge).await?;
        }
        Ok(())
    }
}
