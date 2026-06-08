use super::connector::FlowgraphConnector;
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
        let src = self.location(src_id)?;
        let dst = self.location(dst_id)?;
        let edge = if src.domain == dst.domain {
            match src.domain {
                DomainLocation::Normal => {
                    let (src, dst) = self.get_two_typed_wrapped_blocks_mut(src_id, dst_id)?;
                    Self::stream_ports_edge(src_port(&mut src.kernel), dst_port(&mut dst.kernel))
                }
                DomainLocation::Local(_) => {
                    let (src, dst) = Self::same_local_stream_locations(src, dst, false)?;
                    FlowgraphConnector::new(self)
                        .local_local_stream_edge_async::<KS, KD, B, FS, FD>(
                            src, src_port, dst, dst_port,
                        )
                        .await?
                }
            }
        } else {
            FlowgraphConnector::new(self)
                .cross_domain_stream_edge_async::<KS, KD, B, FS, FD>(src, src_port, dst, dst_port)
                .await?
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
        let src = self.location(src_id)?;
        let dst = self.location(dst_id)?;
        let (src, dst) = Self::same_local_stream_locations(src, dst, false)?;
        let edge = FlowgraphConnector::new(self)
            .local_local_stream_edge_async::<KS, KD, B, FS, FD>(src, src_port, dst, dst_port)
            .await?;
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

        let src = self.location(src_block_id)?;
        let dst = self.location(dst_block_id)?;
        let local_only = src.domain == dst.domain && src.domain.is_local();
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

        let src = self.location(src_block_id)?;
        let dst = self.location(dst_block_id)?;
        Self::same_local_stream_locations(src, dst, true)?;

        let edge = Edge::new(src_block_id, src_port_id, dst_block_id, dst_port_id);
        self.validate_stream_edge_ports(&edge).await?;
        self.stream_edges.push(StreamEdge::from_edge(edge, true));
        Ok(())
    }

    async fn validate_stream_output_port(
        &mut self,
        block_id: BlockId,
        port_id: &PortId,
    ) -> Result<(), Error> {
        let location = self.location(block_id)?;
        let port_id = port_id.clone();
        self.with_block_mut(location, move |block| {
            block.stream_output(&port_id).map_err(|e| match e {
                Error::InvalidStreamPort(_, port) => {
                    Error::InvalidStreamPort(BlockPortCtx::Id(block_id), port)
                }
                other => other,
            })?;
            Ok(())
        })
        .await
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
        let location = self.location(block_id)?;
        let port_id = port_id.clone();
        self.with_block_mut(location, move |block| {
            block.stream_input(&port_id).map_err(|e| match e {
                Error::InvalidStreamPort(_, port) => {
                    Error::InvalidStreamPort(BlockPortCtx::Id(block_id), port)
                }
                other => other,
            })?;
            Ok(())
        })
        .await
    }
}
