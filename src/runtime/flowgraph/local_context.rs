use super::*;

/// Handle for a local scheduling domain inside a [`Flowgraph`].
///
/// Local domains run their blocks on a dedicated single-thread executor. They
/// are used for blocks or buffers that are not `Send`, and for blocks marked
/// as blocking. Stream connections with local-only buffers can only connect
/// blocks inside the same local domain.
#[derive(Debug, Eq, PartialEq)]
pub struct LocalDomain<LS = BasicLocalScheduler> {
    pub(super) flowgraph_id: FlowgraphId,
    pub(super) domain_id: usize,
    pub(super) _marker: PhantomData<fn() -> LS>,
}

impl<LS> Copy for LocalDomain<LS> {}

impl<LS> Clone for LocalDomain<LS> {
    fn clone(&self) -> Self {
        *self
    }
}

pub(super) struct LocalDomainContextEntry {
    pub(super) block_id: BlockId,
    pub(super) placement: BlockPlacement,
    pub(super) inbox: BlockEndpoint,
    pub(super) message_inputs: &'static [&'static str],
    pub(super) message_outputs: &'static [&'static str],
}

struct LocalDomainContextInner<'a> {
    flowgraph_id: FlowgraphId,
    domain_id: usize,
    domain_inbox: LocalDomainInbox,
    next_block_id: usize,
    next_local_id: usize,
    entries: Vec<LocalDomainContextEntry>,
    stream_edges: Vec<StreamEdge>,
    message_edges: Vec<Edge>,
    state: &'a mut LocalDomainState,
}

/// Builder context for constructing blocks directly inside a local domain.
///
/// Blocks added through this context are constructed on the local-domain
/// thread/worker, so their state does not have to be `Send`.
pub struct LocalDomainContext<'a, LS = BasicLocalScheduler> {
    inner: RefCell<LocalDomainContextInner<'a>>,
    scheduler: &'a LS,
}

impl<'a, LS: LocalScheduler> LocalDomainContext<'a, LS> {
    pub(super) fn new(
        flowgraph_id: FlowgraphId,
        domain_id: usize,
        domain_inbox: LocalDomainInbox,
        next_block_id: usize,
        next_local_id: usize,
        state: &'a mut LocalDomainState,
        scheduler: &'a LS,
    ) -> Self {
        Self {
            inner: RefCell::new(LocalDomainContextInner {
                flowgraph_id,
                domain_id,
                domain_inbox,
                next_block_id,
                next_local_id,
                entries: Vec::new(),
                stream_edges: Vec::new(),
                message_edges: Vec::new(),
                state,
            }),
            scheduler,
        }
    }

    /// Spawn a non-`Send` task on this local domain's scheduler.
    ///
    /// The scheduler is owned by the local domain and is reused when the
    /// flowgraph later starts running, so a task whose handle is retained or
    /// detached can continue beyond this builder closure. Dropping the returned
    /// task follows the cancellation semantics of the selected local scheduler.
    pub fn spawn<T: 'static>(
        &self,
        future: impl std::future::Future<Output = T> + 'static,
    ) -> LS::Task<T> {
        self.scheduler.spawn(future)
    }

    /// Spawn a non-`Send` task and detach it immediately.
    pub fn spawn_background<T: 'static>(
        &self,
        future: impl std::future::Future<Output = T> + 'static,
    ) {
        let task = self.scheduler.spawn(future);
        self.scheduler.detach(task);
    }

    pub(super) fn take_entries(
        &self,
    ) -> (Vec<LocalDomainContextEntry>, Vec<StreamEdge>, Vec<Edge>) {
        let mut inner = self.inner.borrow_mut();
        (
            std::mem::take(&mut inner.entries),
            std::mem::take(&mut inner.stream_edges),
            std::mem::take(&mut inner.message_edges),
        )
    }

    pub(super) fn rollback_entries(
        &self,
        entries: &[LocalDomainContextEntry],
    ) -> Result<(), Error> {
        let mut inner = self.inner.borrow_mut();
        let mut result = Ok(());
        for entry in entries.iter().rev() {
            let BlockPlacement::Local { local_id, .. } = entry.placement else {
                continue;
            };
            if let Err(e) = inner.state.remove_block(local_id, entry.block_id)
                && result.is_ok()
            {
                result = Err(e);
            }
        }
        result
    }

    pub(crate) fn validate_block_ref<K>(&self, block: &BlockRef<K>) -> Result<(), Error> {
        let inner = self.inner.borrow();
        if block.flowgraph_id != inner.flowgraph_id {
            return Err(Error::ValidationError(format!(
                "block {:?} belongs to another flowgraph",
                block.id
            )));
        }

        let BlockPlacement::Local {
            domain_id,
            local_id,
        } = block.placement
        else {
            return Err(Error::ValidationError(
                "local-domain context requires local blocks".to_string(),
            ));
        };
        if domain_id != inner.domain_id
            || inner.state.local_id_for_block(block.id) != Some(local_id)
        {
            return Err(Error::InvalidBlock(block.id));
        }
        Ok(())
    }

    /// Add a block to this local domain.
    pub fn add<K>(&self, block: K) -> BlockRef<K>
    where
        K: Kernel + KernelInterface + 'static,
    {
        let mut inner = self.inner.borrow_mut();
        let block_id = BlockId(inner.next_block_id);
        inner.next_block_id += 1;
        let local_id = inner.next_local_id;
        inner.next_local_id += 1;
        let placement = BlockPlacement::Local {
            domain_id: inner.domain_id,
            local_id,
        };

        let external = BlockEndpoint::domain_proxy(inner.domain_inbox.clone(), block_id);
        let mut block = LocalWrappedKernel::new_local_with_external(block, block_id, external);
        block
            .meta
            .set_instance_name(format!("{}-{}", K::type_name(), block_id.0));
        let inbox = block.inbox();
        inner
            .state
            .insert_block(local_id, Box::new(block))
            .expect("failed to insert local-domain block");
        inner.entries.push(LocalDomainContextEntry {
            block_id,
            placement,
            inbox,
            message_inputs: K::message_inputs(),
            message_outputs: K::message_outputs(),
        });
        BlockRef {
            id: block_id,
            flowgraph_id: inner.flowgraph_id,
            placement,
            _marker: PhantomData,
        }
    }

    /// Connect local-only stream ports between blocks in this domain context.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn stream_local<KS, KD, B, FS, FD>(
        &self,
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

    /// Asynchronously connect local-only stream ports between blocks in this domain context.
    pub async fn stream_local_async<KS, KD, B, FS, FD>(
        &self,
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
        let mut inner = self.inner.borrow_mut();
        if src_block.flowgraph_id != inner.flowgraph_id {
            return Err(Error::ValidationError(format!(
                "block {:?} belongs to another flowgraph",
                src_block.id
            )));
        }
        if dst_block.flowgraph_id != inner.flowgraph_id {
            return Err(Error::ValidationError(format!(
                "block {:?} belongs to another flowgraph",
                dst_block.id
            )));
        }

        let (
            BlockPlacement::Local {
                domain_id: src_domain,
                local_id: src_local,
            },
            BlockPlacement::Local {
                domain_id: dst_domain,
                local_id: dst_local,
            },
        ) = (src_block.placement, dst_block.placement)
        else {
            return Err(Error::ValidationError(
                "local-domain context stream connections require local blocks".to_string(),
            ));
        };

        if src_domain != inner.domain_id || dst_domain != inner.domain_id {
            return Err(Error::ValidationError(
                "local-domain context stream connections require blocks in this domain".to_string(),
            ));
        }

        let edge = {
            let (src, dst) = Flowgraph::two_local_state_kernels_mut::<KS, KD>(
                inner.state,
                (src_local, src_block.id),
                (dst_local, dst_block.id),
            )?;
            Flowgraph::stream_ports_edge(src_port(src), dst_port(dst))
        };
        inner.stream_edges.push(StreamEdge::from_edge(edge, true));
        Ok(())
    }

    /// Same-domain stream alias for local-domain contexts.
    pub async fn stream_async<KS, KD, B, FS, FD>(
        &self,
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
        self.stream_local_async(src_block, src_port, dst_block, dst_port)
            .await
    }

    /// Connect message ports between local blocks in this domain context.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn message(
        &self,
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

    /// Asynchronously connect message ports between local blocks in this domain context.
    pub async fn message_async(
        &self,
        src_block_id: impl Into<BlockId>,
        src_port_id: impl Into<PortId>,
        dst_block_id: impl Into<BlockId>,
        dst_port_id: impl Into<PortId>,
    ) -> Result<(), Error> {
        let src_block_id = src_block_id.into();
        let src_port_id = src_port_id.into();
        let dst_block_id = dst_block_id.into();
        let dst_port_id = dst_port_id.into();
        let mut inner = self.inner.borrow_mut();

        let src_local = inner
            .state
            .local_id_for_block(src_block_id)
            .ok_or(Error::InvalidBlock(src_block_id))?;
        let dst_local = inner
            .state
            .local_id_for_block(dst_block_id)
            .ok_or(Error::InvalidBlock(dst_block_id))?;

        let dst_block = inner.state.block(dst_local, dst_block_id)?;
        if !dst_block.message_inputs().contains(&dst_port_id.name()) {
            return Err(Error::InvalidMessagePort(
                BlockPortCtx::Id(dst_block_id),
                dst_port_id.clone(),
            ));
        }
        let src_block = inner.state.block(src_local, src_block_id)?;
        if !src_block.message_outputs().contains(&src_port_id.name()) {
            return Err(Error::InvalidMessagePort(
                BlockPortCtx::Id(src_block_id),
                src_port_id.clone(),
            ));
        }
        inner.message_edges.push(Edge::new(
            src_block_id,
            src_port_id,
            dst_block_id,
            dst_port_id,
        ));
        Ok(())
    }
}
