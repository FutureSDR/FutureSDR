use super::*;

impl Flowgraph {
    pub(super) fn stream_ports_edge<B: BufferWriter>(
        src_port: &mut B,
        dst_port: &mut B::Reader,
    ) -> Edge {
        Edge::new(
            src_port.block_id(),
            src_port.port_id(),
            dst_port.block_id(),
            dst_port.port_id(),
        )
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

    async fn local_local_stream_edge_async<KS, KD, B, FS, FD>(
        &self,
        src: LocalEndpoint,
        src_port: FS,
        dst: LocalEndpoint,
        dst_port: FD,
    ) -> Result<Edge, Error>
    where
        KS: 'static,
        KD: 'static,
        B: BufferWriter,
        FS: FnOnce(&mut KS) -> &mut B + Send + 'static,
        FD: FnOnce(&mut KD) -> &mut B::Reader + Send + 'static,
    {
        if src.domain_id != dst.domain_id {
            return Err(Error::ValidationError(
                "stream connections between different local domains are not supported".to_string(),
            ));
        }
        let domain = self
            .local_domains
            .get(src.domain_id)
            .ok_or(Error::InvalidBlock(src.block_id))?;
        domain
            .exec(move |state| {
                let result = (|| {
                    let (src, dst) = Self::two_local_state_kernels_mut::<KS, KD>(
                        state,
                        (src.local_id, src.block_id),
                        (dst.local_id, dst.block_id),
                    )?;
                    Ok(Self::stream_ports_edge(src_port(src), dst_port(dst)))
                })();
                Box::pin(futures::future::ready(result))
            })
            .await
    }

    async fn typed_stream_output_port_id_async<KS, B, FS>(
        &mut self,
        endpoint: StreamEndpoint,
        src_port: FS,
    ) -> Result<PortId, Error>
    where
        KS: 'static,
        B: BufferWriter,
        FS: FnOnce(&mut KS) -> &mut B + Send + 'static,
    {
        match endpoint {
            StreamEndpoint::Normal(block_id) => {
                let block = self.get_typed_wrapped_block_mut_by_id::<KS>(block_id)?;
                Ok(src_port(&mut block.kernel).port_id())
            }
            StreamEndpoint::Local(endpoint) => {
                let domain = self
                    .local_domains
                    .get(endpoint.domain_id)
                    .ok_or(Error::InvalidBlock(endpoint.block_id))?;
                domain
                    .exec(move |state| {
                        let result = (|| {
                            let block = Self::local_state_kernel_mut::<KS>(
                                state,
                                endpoint.local_id,
                                endpoint.block_id,
                            )?;
                            Ok(src_port(block).port_id())
                        })();
                        Box::pin(futures::future::ready(result))
                    })
                    .await
            }
        }
    }

    async fn typed_stream_input_port_id_async<KD, B, FD>(
        &mut self,
        endpoint: StreamEndpoint,
        dst_port: FD,
    ) -> Result<PortId, Error>
    where
        KD: 'static,
        B: BufferWriter,
        FD: FnOnce(&mut KD) -> &mut B::Reader + Send + 'static,
    {
        match endpoint {
            StreamEndpoint::Normal(block_id) => {
                let block = self.get_typed_wrapped_block_mut_by_id::<KD>(block_id)?;
                Ok(dst_port(&mut block.kernel).port_id())
            }
            StreamEndpoint::Local(endpoint) => {
                let domain = self
                    .local_domains
                    .get(endpoint.domain_id)
                    .ok_or(Error::InvalidBlock(endpoint.block_id))?;
                domain
                    .exec(move |state| {
                        let result = (|| {
                            let block = Self::local_state_kernel_mut::<KD>(
                                state,
                                endpoint.local_id,
                                endpoint.block_id,
                            )?;
                            Ok(dst_port(block).port_id())
                        })();
                        Box::pin(futures::future::ready(result))
                    })
                    .await
            }
        }
    }

    async fn cross_domain_stream_edge_async<KS, KD, B, FS, FD>(
        &mut self,
        src: StreamEndpoint,
        src_port: FS,
        dst: StreamEndpoint,
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
            src.block_id(),
            src_port_id,
            dst.block_id(),
            dst_port_id,
        ))
    }

    fn connect_normal_normal_stream_dyn(
        &mut self,
        src_block_id: BlockId,
        src_port_id: &PortId,
        dst_block_id: BlockId,
        dst_port_id: &PortId,
    ) -> Result<Edge, Error> {
        let (src_slot, dst_slot) = self.two_block_entries_mut(src_block_id, dst_block_id)?;
        let src_block = src_slot
            .block
            .as_mut()
            .map(Box::as_mut)
            .ok_or(Error::LockError)?;
        let dst_block = dst_slot
            .block
            .as_mut()
            .map(Box::as_mut)
            .ok_or(Error::LockError)?;
        Self::connect_stream_ports_dyn(
            src_block_id,
            src_port_id,
            src_block,
            dst_block_id,
            dst_port_id,
            dst_block,
        )
    }

    async fn connect_local_local_stream_dyn_async(
        &mut self,
        src: LocalEndpoint,
        src_port_id: PortId,
        dst: LocalEndpoint,
        dst_port_id: PortId,
    ) -> Result<Edge, Error> {
        if src.domain_id != dst.domain_id {
            return Err(Error::ValidationError(
                "stream connections between different local domains are not supported".to_string(),
            ));
        }
        let domain = self
            .local_domains
            .get(src.domain_id)
            .ok_or(Error::InvalidBlock(src.block_id))?;
        domain
            .exec(move |state| {
                let result = (|| {
                    let (src_block, dst_block) = state.two_blocks_mut(
                        (src.local_id, src.block_id),
                        (dst.local_id, dst.block_id),
                    )?;
                    Self::connect_stream_ports_dyn(
                        src.block_id,
                        &src_port_id,
                        src_block,
                        dst.block_id,
                        &dst_port_id,
                        dst_block,
                    )
                })();
                Box::pin(futures::future::ready(result))
            })
            .await
    }

    async fn take_stream_output_send_token(
        &mut self,
        endpoint: StreamEndpoint,
        port_id: &PortId,
    ) -> Result<Box<dyn DynSendBufferWriterToken>, Error> {
        match endpoint {
            StreamEndpoint::Normal(block_id) => {
                let writer = self
                    .raw_block_mut(block_id)?
                    .stream_output(port_id)
                    .map_err(|e| match e {
                        Error::InvalidStreamPort(_, port) => {
                            Error::InvalidStreamPort(BlockPortCtx::Id(block_id), port)
                        }
                        o => o,
                    })?;
                writer.take_send_token().map_err(|e| match e {
                    Error::InvalidStreamPort(_, port) => {
                        Error::InvalidStreamPort(BlockPortCtx::Id(block_id), port)
                    }
                    o => o,
                })
            }
            StreamEndpoint::Local(endpoint) => {
                let inbox = self
                    .local_domains
                    .get(endpoint.domain_id)
                    .ok_or(Error::InvalidBlock(endpoint.block_id))?
                    .inbox();
                let port_id = port_id.clone();
                inbox
                    .exec(move |state| {
                        let result = (|| {
                            let block = state.block_mut(endpoint.local_id, endpoint.block_id)?;
                            let writer = block.stream_output(&port_id).map_err(|e| match e {
                                Error::InvalidStreamPort(_, port) => Error::InvalidStreamPort(
                                    BlockPortCtx::Id(endpoint.block_id),
                                    port,
                                ),
                                o => o,
                            })?;
                            writer.take_send_token().map_err(|e| match e {
                                Error::InvalidStreamPort(_, port) => Error::InvalidStreamPort(
                                    BlockPortCtx::Id(endpoint.block_id),
                                    port,
                                ),
                                o => o,
                            })
                        })();
                        Box::pin(futures::future::ready(result))
                    })
                    .await
            }
        }
    }

    async fn replace_stream_output_send_token(
        &mut self,
        endpoint: StreamEndpoint,
        port_id: &PortId,
        token: Box<dyn DynSendBufferWriterToken>,
    ) -> Result<(), Error> {
        match endpoint {
            StreamEndpoint::Normal(block_id) => {
                let writer = self
                    .raw_block_mut(block_id)?
                    .stream_output(port_id)
                    .map_err(|e| match e {
                        Error::InvalidStreamPort(_, port) => {
                            Error::InvalidStreamPort(BlockPortCtx::Id(block_id), port)
                        }
                        o => o,
                    })?;
                writer.replace_send_token(token).map_err(|e| match e {
                    Error::InvalidStreamPort(_, port) => {
                        Error::InvalidStreamPort(BlockPortCtx::Id(block_id), port)
                    }
                    o => o,
                })
            }
            StreamEndpoint::Local(endpoint) => {
                let inbox = self
                    .local_domains
                    .get(endpoint.domain_id)
                    .ok_or(Error::InvalidBlock(endpoint.block_id))?
                    .inbox();
                let port_id = port_id.clone();
                inbox
                    .exec(move |state| {
                        let result = (|| {
                            let block = state.block_mut(endpoint.local_id, endpoint.block_id)?;
                            let writer = block.stream_output(&port_id).map_err(|e| match e {
                                Error::InvalidStreamPort(_, port) => Error::InvalidStreamPort(
                                    BlockPortCtx::Id(endpoint.block_id),
                                    port,
                                ),
                                o => o,
                            })?;
                            writer.replace_send_token(token).map_err(|e| match e {
                                Error::InvalidStreamPort(_, port) => Error::InvalidStreamPort(
                                    BlockPortCtx::Id(endpoint.block_id),
                                    port,
                                ),
                                o => o,
                            })
                        })();
                        Box::pin(futures::future::ready(result))
                    })
                    .await
            }
        }
    }

    async fn connect_send_token_to_input(
        &mut self,
        token: Arc<async_lock::Mutex<Option<Box<dyn DynSendBufferWriterToken>>>>,
        src_block_id: BlockId,
        dst: StreamEndpoint,
        dst_port_id: PortId,
    ) -> Result<(), Error> {
        match dst {
            StreamEndpoint::Normal(dst_block_id) => {
                let mut token = token.lock().await;
                let token = token.as_mut().ok_or(Error::LockError)?;
                let dst_block = self.raw_block_mut(dst_block_id)?;
                let reader = dst_block.stream_input(&dst_port_id).map_err(|e| match e {
                    Error::InvalidStreamPort(_, port) => {
                        Error::InvalidStreamPort(BlockPortCtx::Id(dst_block_id), port)
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
            StreamEndpoint::Local(dst) => {
                let inbox = self
                    .local_domains
                    .get(dst.domain_id)
                    .ok_or(Error::InvalidBlock(dst.block_id))?
                    .inbox();
                inbox
                    .exec(move |state| {
                        Box::pin(async move {
                            let mut token = token.lock().await;
                            let token = token.as_mut().ok_or(Error::LockError)?;
                            let dst_block = state.block_mut(dst.local_id, dst.block_id)?;
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
        src: StreamEndpoint,
        src_port_id: PortId,
        dst: StreamEndpoint,
        dst_port_id: PortId,
    ) -> Result<Edge, Error> {
        let src_block_id = src.block_id();
        let dst_block_id = dst.block_id();
        let token = self
            .take_stream_output_send_token(src, &src_port_id)
            .await?;
        let token = Arc::new(async_lock::Mutex::new(Some(token)));

        let connect_result = self
            .connect_send_token_to_input(Arc::clone(&token), src_block_id, dst, dst_port_id.clone())
            .await
            .map(|()| Edge::new(src_block_id, src_port_id.clone(), dst_block_id, dst_port_id));

        let token = token.lock().await.take().ok_or(Error::LockError)?;
        self.replace_stream_output_send_token(src, &src_port_id, token)
            .await?;
        connect_result
    }

    /// Connect stream ports through typed block handles owned by this flowgraph.
    ///
    /// This is the typed block-level stream API used by the
    /// [`connect`](crate::runtime::macros::connect) macro.
    ///
    /// The selected writer must be send-capable and default-constructible. Use
    /// [`Flowgraph::stream_local`] for local-only buffers in a local domain.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn stream<KS, KD, B, FS, FD>(
        &mut self,
        src_block: &BlockRef<KS>,
        src_port: FS,
        dst_block: &BlockRef<KD>,
        dst_port: FD,
    ) -> Result<(), Error>
    where
        KS: 'static,
        KD: 'static,
        B: SendBufferWriter + Default + 'static,
        FS: FnOnce(&mut KS) -> &mut B + Send + 'static,
        FD: FnOnce(&mut KD) -> &mut B::Reader + Send + 'static,
    {
        crate::runtime::block_on(
            self.stream_async::<KS, KD, B, FS, FD>(src_block, src_port, dst_block, dst_port),
        )
    }

    /// Async counterpart to [`Flowgraph::stream`].
    pub async fn stream_async<KS, KD, B, FS, FD>(
        &mut self,
        src_block: &BlockRef<KS>,
        src_port: FS,
        dst_block: &BlockRef<KD>,
        dst_port: FD,
    ) -> Result<(), Error>
    where
        KS: 'static,
        KD: 'static,
        B: SendBufferWriter + Default + 'static,
        FS: FnOnce(&mut KS) -> &mut B + Send + 'static,
        FD: FnOnce(&mut KD) -> &mut B::Reader + Send + 'static,
    {
        self.validate_block_ref(src_block)?;
        self.validate_block_ref(dst_block)?;
        let src_id = src_block.id;
        let dst_id = dst_block.id;
        let edge = match Self::stream_plan(src_id, src_block.placement, dst_id, dst_block.placement)
        {
            StreamPlan::NormalNormal {
                src: src_id,
                dst: dst_id,
            } => {
                let (src, dst) = self.get_two_typed_wrapped_blocks_mut(src_id, dst_id)?;
                Self::stream_ports_edge(src_port(&mut src.kernel), dst_port(&mut dst.kernel))
            }
            StreamPlan::LocalLocalSame { src, dst } => {
                self.local_local_stream_edge_async::<KS, KD, B, FS, FD>(
                    src, src_port, dst, dst_port,
                )
                .await?
            }
            StreamPlan::LocalLocalCross { src, dst } => {
                self.cross_domain_stream_edge_async::<KS, KD, B, FS, FD>(
                    StreamEndpoint::Local(src),
                    src_port,
                    StreamEndpoint::Local(dst),
                    dst_port,
                )
                .await?
            }
            StreamPlan::LocalToNormal { src, dst } => {
                self.cross_domain_stream_edge_async::<KS, KD, B, FS, FD>(
                    StreamEndpoint::Local(src),
                    src_port,
                    StreamEndpoint::Normal(dst),
                    dst_port,
                )
                .await?
            }
            StreamPlan::NormalToLocal { src, dst } => {
                self.cross_domain_stream_edge_async::<KS, KD, B, FS, FD>(
                    StreamEndpoint::Normal(src),
                    src_port,
                    StreamEndpoint::Local(dst),
                    dst_port,
                )
                .await?
            }
        };
        self.stream_edges.push(StreamEdge::from_edge(edge, false));
        Ok(())
    }

    /// Connect local-only stream ports through typed block handles owned by this flowgraph.
    ///
    /// This only accepts two local-domain blocks in the same [`LocalDomain`].
    /// Use this for non-`Send` stream buffers such as
    /// [`LocalCpuWriter`](crate::runtime::buffer::LocalCpuWriter).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn stream_local<KS, KD, B, FS, FD>(
        &mut self,
        src_block: &BlockRef<KS>,
        src_port: FS,
        dst_block: &BlockRef<KD>,
        dst_port: FD,
    ) -> Result<(), Error>
    where
        KS: 'static,
        KD: 'static,
        B: BufferWriter + 'static,
        FS: FnOnce(&mut KS) -> &mut B + Send + 'static,
        FD: FnOnce(&mut KD) -> &mut B::Reader + Send + 'static,
    {
        crate::runtime::block_on(
            self.stream_local_async::<KS, KD, B, FS, FD>(src_block, src_port, dst_block, dst_port),
        )
    }

    /// Async counterpart to [`Flowgraph::stream_local`].
    pub async fn stream_local_async<KS, KD, B, FS, FD>(
        &mut self,
        src_block: &BlockRef<KS>,
        src_port: FS,
        dst_block: &BlockRef<KD>,
        dst_port: FD,
    ) -> Result<(), Error>
    where
        KS: 'static,
        KD: 'static,
        B: BufferWriter + 'static,
        FS: FnOnce(&mut KS) -> &mut B + Send + 'static,
        FD: FnOnce(&mut KD) -> &mut B::Reader + Send + 'static,
    {
        self.validate_block_ref(src_block)?;
        self.validate_block_ref(dst_block)?;
        let src_id = src_block.id;
        let dst_id = dst_block.id;
        let edge = match Self::stream_plan(src_id, src_block.placement, dst_id, dst_block.placement)
        {
            StreamPlan::LocalLocalSame { src, dst } => {
                self.local_local_stream_edge_async::<KS, KD, B, FS, FD>(
                    src, src_port, dst, dst_port,
                )
                .await?
            }
            StreamPlan::LocalLocalCross { .. } => {
                return Err(Error::ValidationError(
                    "stream connections between different local domains are not supported"
                        .to_string(),
                ));
            }
            _ => {
                return Err(Error::ValidationError(
                    "local stream connections require source and destination blocks in the same local domain"
                        .to_string(),
                ));
            }
        };
        self.stream_edges.push(StreamEdge::from_edge(edge, true));
        Ok(())
    }

    /// Connect stream ports by block id and port name.
    ///
    /// This dynamic API skips the compile-time port type checks provided by
    /// [`Flowgraph::stream`]. Port existence is validated immediately; buffer
    /// compatibility is checked when the flowgraph applies the connection at startup.
    ///
    /// Prefer the typed API when the concrete block types are known. The dynamic
    /// API is useful when a runtime option selects between different block
    /// implementations, for example switching a source between hardware and a
    /// file.
    ///
    /// ```
    /// use anyhow::Result;
    /// use futuresdr::blocks::Head;
    /// use futuresdr::blocks::NullSink;
    /// use futuresdr::blocks::NullSource;
    /// use futuresdr::prelude::*;
    ///
    /// fn main() -> Result<()> {
    ///     let mut fg = Flowgraph::new();
    ///
    ///     let src = NullSource::<u8>::new();
    ///     let head = Head::<u8>::new(1234);
    ///     let snk = NullSink::<u8>::new();
    ///
    ///     let src = fg.add(src)?;
    ///     let head = fg.add(head)?;
    ///
    ///     // dynamic stream connection by port name
    ///     fg.stream_dyn(src, "output", head, "input")?;
    ///     // typed connection through the `connect!` macro
    ///     connect!(fg, head > snk);
    ///
    ///     Runtime::new().run(fg)?;
    ///     Ok(())
    /// }
    /// ```
    #[cfg(not(target_arch = "wasm32"))]
    pub fn stream_dyn(
        &mut self,
        src_block_id: impl Into<BlockId>,
        src_port_id: impl Into<PortId>,
        dst_block_id: impl Into<BlockId>,
        dst_port_id: impl Into<PortId>,
    ) -> Result<(), Error> {
        crate::runtime::block_on(self.stream_dyn_async(
            src_block_id,
            src_port_id,
            dst_block_id,
            dst_port_id,
        ))
    }

    /// Async counterpart to [`Flowgraph::stream_dyn`].
    pub async fn stream_dyn_async(
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

        let local_only = matches!(
            self.stream_plan_by_id(src_block_id, dst_block_id)?,
            StreamPlan::LocalLocalSame { .. }
        );
        let edge = Edge::new(src_block_id, src_port_id, dst_block_id, dst_port_id);
        self.validate_stream_edge_ports(&edge).await?;
        self.stream_edges
            .push(StreamEdge::from_edge(edge, local_only));
        Ok(())
    }

    /// Connect local-only stream ports without static port type checks.
    ///
    /// This only accepts two local-domain blocks in the same [`LocalDomain`].
    /// Use [`Flowgraph::stream_dyn`] for send-capable/default dynamic stream
    /// connections that involve normal runtime blocks.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn stream_local_dyn(
        &mut self,
        src_block_id: impl Into<BlockId>,
        src_port_id: impl Into<PortId>,
        dst_block_id: impl Into<BlockId>,
        dst_port_id: impl Into<PortId>,
    ) -> Result<(), Error> {
        crate::runtime::block_on(self.stream_local_dyn_async(
            src_block_id,
            src_port_id,
            dst_block_id,
            dst_port_id,
        ))
    }

    /// Async counterpart to [`Flowgraph::stream_local_dyn`].
    pub async fn stream_local_dyn_async(
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

        match self.stream_plan_by_id(src_block_id, dst_block_id)? {
            StreamPlan::LocalLocalSame { .. } => {}
            StreamPlan::LocalLocalCross { .. } => {
                return Err(Error::ValidationError(
                    "stream connections between different local domains are not supported"
                        .to_string(),
                ));
            }
            _ => {
                return Err(Error::ValidationError(
                    "local dynamic stream connections require source and destination blocks in the same local domain"
                        .to_string(),
                ));
            }
        };

        let edge = Edge::new(src_block_id, src_port_id, dst_block_id, dst_port_id);
        self.validate_stream_edge_ports(&edge).await?;
        self.stream_edges.push(StreamEdge::from_edge(edge, true));
        Ok(())
    }

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
        self.validate_message_edge(&edge).await?;
        self.message_edges.push(edge);
        Ok(())
    }

    async fn validate_stream_output_port(
        &mut self,
        block_id: BlockId,
        port_id: &PortId,
    ) -> Result<(), Error> {
        match self.placement(block_id)? {
            BlockPlacement::Normal => {
                let block = self.raw_block_mut(block_id)?;
                let _writer = block.stream_output(port_id).map_err(|e| match e {
                    Error::InvalidStreamPort(_, port) => {
                        Error::InvalidStreamPort(BlockPortCtx::Id(block_id), port)
                    }
                    other => other,
                })?;
            }
            BlockPlacement::Local {
                domain_id,
                local_id,
            } => {
                let port_id = port_id.clone();
                self.local_domains[domain_id]
                    .exec(move |state| {
                        let result = (|| {
                            let block = state.block_mut(local_id, block_id)?;
                            let _writer = block.stream_output(&port_id).map_err(|e| match e {
                                Error::InvalidStreamPort(_, port) => {
                                    Error::InvalidStreamPort(BlockPortCtx::Id(block_id), port)
                                }
                                other => other,
                            })?;
                            Ok(())
                        })();
                        Box::pin(futures::future::ready(result))
                    })
                    .await?;
            }
        }
        Ok(())
    }

    async fn validate_stream_edge_ports(&mut self, edge: &Edge) -> Result<(), Error> {
        self.validate_stream_output_port(edge.src_block, &edge.src_port)
            .await?;
        self.validate_stream_input_port(edge.dst_block, &edge.dst_port)
            .await
    }

    async fn validate_stream_input_port(
        &mut self,
        block_id: BlockId,
        port_id: &PortId,
    ) -> Result<(), Error> {
        match self.placement(block_id)? {
            BlockPlacement::Normal => {
                let block = self.raw_block_mut(block_id)?;
                let _reader = block.stream_input(port_id).map_err(|e| match e {
                    Error::InvalidStreamPort(_, port) => {
                        Error::InvalidStreamPort(BlockPortCtx::Id(block_id), port)
                    }
                    other => other,
                })?;
                Ok(())
            }
            BlockPlacement::Local {
                domain_id,
                local_id,
            } => {
                let port_id = port_id.clone();
                self.local_domains[domain_id]
                    .exec(move |state| {
                        let result = (|| {
                            let block = state.block_mut(local_id, block_id)?;
                            let _reader = block.stream_input(&port_id).map_err(|e| match e {
                                Error::InvalidStreamPort(_, port) => {
                                    Error::InvalidStreamPort(BlockPortCtx::Id(block_id), port)
                                }
                                other => other,
                            })?;
                            Ok(())
                        })();
                        Box::pin(futures::future::ready(result))
                    })
                    .await
            }
        }
    }

    async fn apply_stream_edge(&mut self, edge: &Edge) -> Result<(), Error> {
        match self.stream_plan_by_id(edge.src_block, edge.dst_block)? {
            StreamPlan::NormalNormal { src, dst } => {
                self.connect_normal_normal_stream_dyn(src, &edge.src_port, dst, &edge.dst_port)?;
            }
            StreamPlan::LocalLocalSame { src, dst } => {
                self.connect_local_local_stream_dyn_async(
                    src,
                    edge.src_port.clone(),
                    dst,
                    edge.dst_port.clone(),
                )
                .await?;
            }
            StreamPlan::LocalLocalCross { src, dst } => {
                self.connect_cross_domain_stream_dyn_async(
                    StreamEndpoint::Local(src),
                    edge.src_port.clone(),
                    StreamEndpoint::Local(dst),
                    edge.dst_port.clone(),
                )
                .await?;
            }
            StreamPlan::LocalToNormal { src, dst } => {
                self.connect_cross_domain_stream_dyn_async(
                    StreamEndpoint::Local(src),
                    edge.src_port.clone(),
                    StreamEndpoint::Normal(dst),
                    edge.dst_port.clone(),
                )
                .await?;
            }
            StreamPlan::NormalToLocal { src, dst } => {
                self.connect_cross_domain_stream_dyn_async(
                    StreamEndpoint::Normal(src),
                    edge.src_port.clone(),
                    StreamEndpoint::Local(dst),
                    edge.dst_port.clone(),
                )
                .await?;
            }
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
        let src_placement = self.placement(edge.src_block)?;

        let dst = self
            .blocks
            .get(edge.dst_block.0)
            .and_then(|entry| entry.inbox.as_ref())
            .cloned()
            .ok_or(Error::InvalidBlock(edge.dst_block))?;
        match src_placement {
            BlockPlacement::Normal => {
                let src_block = self.raw_block_mut(edge.src_block)?;
                src_block.connect_message(&edge.src_port, dst, &edge.dst_port)?;
            }
            BlockPlacement::Local {
                domain_id,
                local_id,
            } => {
                self.local_domains[domain_id]
                    .exec(move |state| {
                        let result = (|| {
                            let src_block = state.block_mut(local_id, edge.src_block)?;
                            src_block.connect_message(&edge.src_port, dst, &edge.dst_port)
                        })();
                        Box::pin(futures::future::ready(result))
                    })
                    .await?;
            }
        }
        Ok(())
    }

    pub(super) async fn apply_message_edges(&mut self, edges: &[Edge]) -> Result<(), Error> {
        for edge in edges.iter().cloned() {
            self.apply_message_edge(edge).await?;
        }
        Ok(())
    }
}
