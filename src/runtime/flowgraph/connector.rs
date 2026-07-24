use std::sync::Arc;

use crate::runtime::BlockId;
use crate::runtime::BlockPortCtx;
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
use crate::runtime::buffer::PortManifest;
use crate::runtime::buffer::ThreadSafeConnect;

use super::Flowgraph;
use super::prepare::ResolvedEdge;
use super::types::BlockLocation;

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
        let src_port_id = PortId::Index(src_port);
        let dst_port_id = PortId::Index(dst_port);

        let reader = dst_block.stream_input(&dst_port_id).map_err(|e| match e {
            Error::InvalidStreamPort(_, port) => {
                Error::InvalidStreamPort(BlockPortCtx::Id(dst_block_id), port)
            }
            o => o,
        })?;
        reader.raise_buffer_requirements(dst_requirements);

        let writer = src_block.stream_output(&src_port_id).map_err(|e| match e {
            Error::InvalidStreamPort(_, port) => {
                Error::InvalidStreamPort(BlockPortCtx::Id(src_block_id), port)
            }
            o => o,
        })?;
        writer.raise_buffer_requirements(src_requirements);

        writer.connect_dyn(reader).map_err(|e| match e {
            Error::InvalidStreamPort(_, port) => {
                Error::InvalidStreamPort(BlockPortCtx::Id(src_block_id), port)
            }
            o => o,
        })?;

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
            .with_typed_kernel_mut::<KS, _>(location, move |kernel| Ok(src_port(kernel).port_id()))
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
            .with_typed_kernel_mut::<KD, _>(location, move |kernel| Ok(dst_port(kernel).port_id()))
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
        let port_id = PortId::Index(port);
        self.flowgraph
            .with_block_mut(location, move |block| {
                let reader = block.stream_input(&port_id).map_err(|e| match e {
                    Error::InvalidStreamPort(_, port) => {
                        Error::InvalidStreamPort(BlockPortCtx::Id(location.block_id), port)
                    }
                    o => o,
                })?;
                reader.raise_buffer_requirements(requirements);
                connect.take_reader(reader).map_err(|e| match e {
                    Error::InvalidStreamPort(_, port) => {
                        Error::InvalidStreamPort(BlockPortCtx::Id(location.block_id), port)
                    }
                    o => o,
                })
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
        let port_id = PortId::Index(port);
        self.flowgraph
            .with_block_mut(location, move |block| {
                let writer = block.stream_output(&port_id).map_err(|e| match e {
                    Error::InvalidStreamPort(_, port) => {
                        Error::InvalidStreamPort(BlockPortCtx::Id(location.block_id), port)
                    }
                    o => o,
                })?;
                writer.raise_buffer_requirements(requirements);
                connect.connect_reader(writer, token).map_err(|e| match e {
                    Error::InvalidStreamPort(_, port) => {
                        Error::InvalidStreamPort(BlockPortCtx::Id(location.block_id), port)
                    }
                    o => o,
                })
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
                let port_id = PortId::Index(port);
                let reader = block.stream_input(&port_id).map_err(|e| match e {
                    Error::InvalidStreamPort(_, port) => {
                        Error::InvalidStreamPort(BlockPortCtx::Id(location.block_id), port)
                    }
                    o => o,
                })?;
                connect.finish_reader(reader, token).map_err(|e| match e {
                    Error::InvalidStreamPort(_, port) => {
                        Error::InvalidStreamPort(BlockPortCtx::Id(location.block_id), port)
                    }
                    o => o,
                })
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

    fn validate_stream_group(
        &self,
        group: &ResolvedStreamGroup,
    ) -> Result<(BufferRequirements, Vec<BufferRequirements>), Error> {
        let src_manifest = self
            .flowgraph
            .stream_output_manifest(group.src_block, group.src_port)?;
        let max_readers = src_manifest.requirements().max_readers().unwrap_or(1);
        if group.dsts.len() > max_readers {
            return Err(Error::ValidationError(format!(
                "stream output {:?}.{} supports at most {} reader(s)",
                group.src_block,
                src_manifest.name(),
                max_readers
            )));
        }

        let src_reader_type = src_manifest.reader_type_id().ok_or_else(|| {
            Error::ValidationError("stream output manifest missing reader type".to_string())
        })?;
        let mut merged = src_manifest.requirements();
        for (dst_block, dst_port) in &group.dsts {
            let dst_manifest = self
                .flowgraph
                .stream_input_manifest(*dst_block, *dst_port)?;
            if src_reader_type != dst_manifest.concrete_type_id() {
                return Err(Error::ValidationError(
                    "dyn BufferReader has wrong type".to_string(),
                ));
            }
            let requirements = dst_manifest.requirements();
            merged.merge(requirements);
        }

        let source_requirements = Self::with_merged_buffer_size(src_manifest, merged);
        let dst_requirements = group
            .dsts
            .iter()
            .map(|(dst_block, dst_port)| {
                let dst_manifest = self
                    .flowgraph
                    .stream_input_manifest(*dst_block, *dst_port)
                    .expect("destination manifest was already validated");
                Self::with_merged_buffer_size(dst_manifest, merged)
            })
            .collect();
        Ok((source_requirements, dst_requirements))
    }

    fn with_merged_buffer_size(
        manifest: &PortManifest,
        merged: BufferRequirements,
    ) -> BufferRequirements {
        let mut requirements = manifest.requirements();
        if let Some(min_items) = merged.min_buffer_size_in_items() {
            requirements.raise_min_buffer_size_in_items(min_items);
        }
        requirements
    }

    async fn apply_stream_group(&mut self, group: &ResolvedStreamGroup) -> Result<(), Error> {
        let (src_requirements, dst_requirements) = self.validate_stream_group(group)?;
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
            let connect = self
                .flowgraph
                .stream_output_manifest(group.src_block, group.src_port)
                .ok()
                .and_then(PortManifest::thread_safe_connect)
                .ok_or_else(|| {
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
