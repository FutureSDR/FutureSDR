use std::sync::Arc;

use super::*;

struct StreamOutputSendTokenLease {
    location: BlockLocation,
    port_id: PortId,
    token: Arc<async_lock::Mutex<Option<Box<dyn DynSendBufferWriterToken>>>>,
}

impl StreamOutputSendTokenLease {
    fn new(
        location: BlockLocation,
        port_id: PortId,
        token: Box<dyn DynSendBufferWriterToken>,
    ) -> Self {
        Self {
            location,
            port_id,
            token: Arc::new(async_lock::Mutex::new(Some(token))),
        }
    }

    fn shared_token(&self) -> Arc<async_lock::Mutex<Option<Box<dyn DynSendBufferWriterToken>>>> {
        Arc::clone(&self.token)
    }

    async fn into_parts(
        self,
    ) -> Result<(BlockLocation, PortId, Box<dyn DynSendBufferWriterToken>), Error> {
        let token = self.token.lock().await.take().ok_or(Error::LockError)?;
        Ok((self.location, self.port_id, token))
    }
}

pub(super) struct FlowgraphConnector<'a> {
    flowgraph: &'a mut Flowgraph,
}

impl<'a> FlowgraphConnector<'a> {
    pub(super) fn new(flowgraph: &'a mut Flowgraph) -> Self {
        Self { flowgraph }
    }

    fn connect_stream_ports_dyn(
        src_block_id: BlockId,
        src_port_id: &PortId,
        src_block: &mut dyn BlockObject,
        dst_block_id: BlockId,
        dst_port_id: &PortId,
        dst_block: &mut dyn BlockObject,
    ) -> Result<Edge, Error> {
        let reader = dst_block.stream_input(dst_port_id).map_err(|e| match e {
            Error::InvalidStreamPort(_, port) => {
                Error::InvalidStreamPort(crate::runtime::BlockPortCtx::Id(dst_block_id), port)
            }
            o => o,
        })?;

        let writer = src_block.stream_output(src_port_id).map_err(|e| match e {
            Error::InvalidStreamPort(_, port) => {
                Error::InvalidStreamPort(crate::runtime::BlockPortCtx::Id(src_block_id), port)
            }
            o => o,
        })?;

        writer.connect_dyn(reader).map_err(|e| match e {
            Error::InvalidStreamPort(_, port) => {
                Error::InvalidStreamPort(crate::runtime::BlockPortCtx::Id(src_block_id), port)
            }
            o => o,
        })?;

        Ok(Edge::new(
            src_block_id,
            src_port_id.clone(),
            dst_block_id,
            dst_port_id.clone(),
        ))
    }

    pub(super) async fn local_local_stream_edge_async<KS, KD, B, FS, FD>(
        &self,
        src: BlockLocation,
        src_port: FS,
        dst: BlockLocation,
        dst_port: FD,
    ) -> Result<Edge, Error>
    where
        KS: 'static,
        KD: 'static,
        B: BufferWriter,
        FS: FnOnce(&mut KS) -> &mut B + Send + 'static,
        FD: FnOnce(&mut KD) -> &mut B::Reader + Send + 'static,
    {
        let (src, dst) = Flowgraph::same_local_stream_locations(src, dst, false)?;
        let DomainLocation::Local(domain_id) = src.domain else {
            unreachable!("same_local_stream_locations ensures a local domain")
        };
        let domain = self
            .flowgraph
            .local_domains
            .get(domain_id)
            .ok_or(Error::InvalidBlock(src.block_id))?;
        domain
            .exec(move |state| {
                let result = (|| {
                    let (src, dst) = Flowgraph::two_local_state_kernels_mut::<KS, KD>(
                        state,
                        (src.domain_slot, src.block_id),
                        (dst.domain_slot, dst.block_id),
                    )?;
                    Ok(Flowgraph::stream_ports_edge(src_port(src), dst_port(dst)))
                })();
                Box::pin(futures::future::ready(result))
            })
            .await
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
        B: BufferWriter,
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
        src_port_id: PortId,
        dst: BlockLocation,
        dst_port_id: PortId,
    ) -> Result<Edge, Error> {
        let src_block_id = src.block_id;
        let dst_block_id = dst.block_id;
        self.flowgraph
            .with_same_domain_two_blocks_mut(src, dst, move |src_block, dst_block| {
                Self::connect_stream_ports_dyn(
                    src_block_id,
                    &src_port_id,
                    src_block,
                    dst_block_id,
                    &dst_port_id,
                    dst_block,
                )
            })
            .await
    }

    async fn lease_stream_output_send_token(
        &mut self,
        location: BlockLocation,
        port_id: &PortId,
    ) -> Result<StreamOutputSendTokenLease, Error> {
        let port_id = port_id.clone();
        let token = self
            .flowgraph
            .with_block_mut(location, {
                let port_id = port_id.clone();
                move |block| {
                    let writer = block.stream_output(&port_id).map_err(|e| match e {
                        Error::InvalidStreamPort(_, port) => {
                            Error::InvalidStreamPort(BlockPortCtx::Id(location.block_id), port)
                        }
                        o => o,
                    })?;
                    writer.take_send_token().map_err(|e| match e {
                        Error::InvalidStreamPort(_, port) => {
                            Error::InvalidStreamPort(BlockPortCtx::Id(location.block_id), port)
                        }
                        o => o,
                    })
                }
            })
            .await?;
        Ok(StreamOutputSendTokenLease::new(location, port_id, token))
    }

    async fn restore_stream_output_send_token(
        &mut self,
        lease: StreamOutputSendTokenLease,
    ) -> Result<(), Error> {
        let (location, port_id, token) = lease.into_parts().await?;
        self.flowgraph
            .with_block_mut(location, move |block| {
                let writer = block.stream_output(&port_id).map_err(|e| match e {
                    Error::InvalidStreamPort(_, port) => {
                        Error::InvalidStreamPort(BlockPortCtx::Id(location.block_id), port)
                    }
                    o => o,
                })?;
                writer.replace_send_token(token).map_err(|e| match e {
                    Error::InvalidStreamPort(_, port) => {
                        Error::InvalidStreamPort(BlockPortCtx::Id(location.block_id), port)
                    }
                    o => o,
                })
            })
            .await
    }

    async fn connect_send_token_to_input(
        &mut self,
        lease: &StreamOutputSendTokenLease,
        src_block_id: BlockId,
        dst: BlockLocation,
        dst_port_id: PortId,
    ) -> Result<(), Error> {
        let token = lease.shared_token();
        match dst.domain {
            DomainLocation::Normal => {
                let mut token = token.lock().await;
                let token = token.as_mut().ok_or(Error::LockError)?;
                let dst_block = block_access::raw_block_mut(&mut self.flowgraph.blocks, dst)?;
                let reader = dst_block.stream_input(&dst_port_id).map_err(|e| match e {
                    Error::InvalidStreamPort(_, port) => {
                        Error::InvalidStreamPort(BlockPortCtx::Id(dst.block_id), port)
                    }
                    o => o,
                })?;
                token.connect_dyn(reader).map_err(|e| match e {
                    Error::InvalidStreamPort(_, port) => {
                        Error::InvalidStreamPort(BlockPortCtx::Id(src_block_id), port)
                    }
                    o => o,
                })
            }
            DomainLocation::Local(domain_id) => {
                let inbox = self
                    .flowgraph
                    .local_domains
                    .get(domain_id)
                    .ok_or(Error::InvalidBlock(dst.block_id))?
                    .inbox();
                inbox
                    .exec(move |state| {
                        Box::pin(async move {
                            let mut token = token.lock().await;
                            let token = token.as_mut().ok_or(Error::LockError)?;
                            let dst_block = state.block_mut(dst.domain_slot, dst.block_id)?;
                            let reader =
                                dst_block.stream_input(&dst_port_id).map_err(|e| match e {
                                    Error::InvalidStreamPort(_, port) => Error::InvalidStreamPort(
                                        BlockPortCtx::Id(dst.block_id),
                                        port,
                                    ),
                                    o => o,
                                })?;
                            token.connect_dyn(reader).map_err(|e| match e {
                                Error::InvalidStreamPort(_, port) => {
                                    Error::InvalidStreamPort(BlockPortCtx::Id(src_block_id), port)
                                }
                                o => o,
                            })
                        })
                    })
                    .await
            }
        }
    }

    async fn connect_cross_domain_stream_dyn_async(
        &mut self,
        src: BlockLocation,
        src_port_id: PortId,
        dst: BlockLocation,
        dst_port_id: PortId,
    ) -> Result<Edge, Error> {
        let src_block_id = src.block_id;
        let dst_block_id = dst.block_id;
        let lease = self
            .lease_stream_output_send_token(src, &src_port_id)
            .await?;

        let connect_result = self
            .connect_send_token_to_input(&lease, src_block_id, dst, dst_port_id.clone())
            .await
            .map(|()| Edge::new(src_block_id, src_port_id.clone(), dst_block_id, dst_port_id));

        self.restore_stream_output_send_token(lease).await?;
        connect_result
    }

    async fn apply_stream_edge(&mut self, edge: &Edge) -> Result<(), Error> {
        let src = self.flowgraph.location(edge.src_block)?;
        let dst = self.flowgraph.location(edge.dst_block)?;

        if src.domain == dst.domain {
            self.connect_same_domain_stream_dyn_async(
                src,
                edge.src_port.clone(),
                dst,
                edge.dst_port.clone(),
            )
            .await?;
        } else {
            self.connect_cross_domain_stream_dyn_async(
                src,
                edge.src_port.clone(),
                dst,
                edge.dst_port.clone(),
            )
            .await?;
        }
        Ok(())
    }

    pub(super) async fn apply_stream_edges(&mut self, edges: &[Edge]) -> Result<(), Error> {
        for edge in edges {
            self.apply_stream_edge(edge).await?;
        }
        Ok(())
    }

    async fn apply_message_edge(&mut self, edge: Edge) -> Result<(), Error> {
        let src = self.flowgraph.location(edge.src_block)?;
        let dst = self
            .flowgraph
            .blocks
            .get(edge.dst_block.0)
            .map(BlockSlot::endpoint)
            .cloned()
            .ok_or(Error::InvalidBlock(edge.dst_block))?;

        self.flowgraph
            .with_block_mut(src, move |src_block| {
                src_block.connect_message(&edge.src_port, dst, &edge.dst_port)
            })
            .await
    }

    pub(super) async fn apply_message_edges(&mut self, edges: &[Edge]) -> Result<(), Error> {
        for edge in edges.iter().cloned() {
            self.apply_message_edge(edge).await?;
        }
        Ok(())
    }
}
