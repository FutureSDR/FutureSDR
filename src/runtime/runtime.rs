#[cfg(not(target_arch = "wasm32"))]
use async_io::block_on;
#[cfg(not(target_arch = "wasm32"))]
use axum::Router;
use futures::channel::mpsc::channel;
use futures::channel::mpsc::Receiver;
use futures::channel::mpsc::Sender;
use futures::channel::oneshot;
use futures::prelude::*;
use futures::FutureExt;
use slab::Slab;
use std::fmt;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::task;
use std::task::Poll;

use crate::runtime;
use crate::runtime::config;
use crate::runtime::scheduler::Scheduler;
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::scheduler::SmolScheduler;
use crate::runtime::scheduler::Task;
#[cfg(target_arch = "wasm32")]
use crate::runtime::scheduler::WasmScheduler;
use crate::runtime::BlockDescription;
use crate::runtime::BlockMessage;
use crate::runtime::ControlPort;
use crate::runtime::Error;
use crate::runtime::Flowgraph;
use crate::runtime::FlowgraphDescription;
use crate::runtime::FlowgraphHandle;
use crate::runtime::FlowgraphMessage;
use crate::runtime::Pmt;

pub struct TaskHandle<'a, T> {
    task: Option<Task<T>>,
    _p: std::marker::PhantomData<&'a ()>,
}

#[cfg(not(target_arch = "wasm32"))]
impl<T> Drop for TaskHandle<'_, T> {
    fn drop(&mut self) {
        self.task.take().unwrap().detach()
    }
}

impl<T> TaskHandle<'_, T> {
    fn new(task: Task<T>) -> Self {
        TaskHandle {
            task: Some(task),
            _p: std::marker::PhantomData,
        }
    }
}

impl<T> std::future::Future for TaskHandle<'_, T> {
    type Output = T;
    fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        self.task.as_mut().unwrap().poll_unpin(cx)
    }
}

/// This is the [Runtime] that runs a [Flowgraph] to completion.
///
/// [Runtime]s are generic over the scheduler used to run the [Flowgraph].
pub struct Runtime<'a, S> {
    scheduler: S,
    flowgraphs: Arc<Mutex<Slab<FlowgraphHandle>>>,
    _control_port: ControlPort,
    _p: std::marker::PhantomData<&'a ()>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Runtime<'_, SmolScheduler> {
    /// Constructs a new [Runtime] using [SmolScheduler::default()] for the [Scheduler].
    pub fn new() -> Self {
        Self::with_custom_routes(Router::new())
    }

    /// Set custom routes for the control port Axum webserver
    pub fn with_custom_routes(routes: Router) -> Self {
        runtime::init();
        let scheduler = SmolScheduler::default();
        let flowgraphs = Arc::new(Mutex::new(Slab::new()));
        let handle = RuntimeHandle {
            flowgraphs: flowgraphs.clone(),
            scheduler: Arc::new(scheduler.clone()),
        };
        Runtime {
            scheduler,
            flowgraphs,
            _control_port: ControlPort::new(handle, routes),
            _p: std::marker::PhantomData,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for Runtime<'_, SmolScheduler> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> Drop for Runtime<'_, S> {
    fn drop(&mut self) {
        debug!("Runtime dropped");
    }
}

#[cfg(target_arch = "wasm32")]
impl Runtime<'_, WasmScheduler> {
    /// Create Runtime
    pub fn new() -> Self {
        runtime::init();
        let flowgraphs = Arc::new(Mutex::new(Slab::new()));
        Runtime {
            scheduler: WasmScheduler,
            flowgraphs,
            _control_port: ControlPort::new(),
            _p: std::marker::PhantomData,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Default for Runtime<'_, WasmScheduler> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, S: Scheduler + Sync> Runtime<'a, S> {
    /// Create a [Runtime] with a given [Scheduler]
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_scheduler(scheduler: S) -> Self {
        Self::with_config(scheduler, Router::new())
    }

    /// Create runtime with given scheduler and Axum routes
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_config(scheduler: S, routes: Router) -> Self {
        runtime::init();

        let flowgraphs = Arc::new(Mutex::new(Slab::new()));
        let handle = RuntimeHandle {
            flowgraphs: flowgraphs.clone(),
            scheduler: Arc::new(scheduler.clone()),
        };
        Runtime {
            scheduler,
            flowgraphs,
            _control_port: ControlPort::new(handle, routes),
            _p: std::marker::PhantomData,
        }
    }

    /// Spawn task on runtime
    pub fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        self.scheduler.spawn(future)
    }

    /// Block thread, waiting for future to complete
    #[cfg(not(target_arch = "wasm32"))]
    pub fn block_on<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> T {
        block_on(self.scheduler.spawn(future))
    }

    /// Spawn task on runtime in background, detaching the handle
    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn_background<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) {
        self.scheduler.spawn(future).detach();
    }

    /// Spawn a blocking task
    ///
    /// This is usually moved in a separate thread.
    pub fn spawn_blocking<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        self.scheduler.spawn_blocking(future)
    }

    /// Spawn a blocking task in the background
    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn_blocking_background<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) {
        self.scheduler.spawn_blocking(future).detach();
    }

    /// Start a [`Flowgraph`] on the [`Runtime`]
    ///
    /// Returns, once the flowgraph is constructed and running.
    pub async fn start<'b>(
        &'a self,
        fg: Flowgraph,
    ) -> (TaskHandle<'b, Result<Flowgraph, Error>>, FlowgraphHandle)
    where
        'a: 'b,
    {
        let queue_size = config::config().queue_size;
        let (fg_inbox, fg_inbox_rx) = channel::<FlowgraphMessage>(queue_size);

        let (tx, rx) = oneshot::channel::<Result<(), Error>>();
        let task = self.scheduler.spawn(run_flowgraph(
            fg,
            self.scheduler.clone(),
            fg_inbox.clone(),
            fg_inbox_rx,
            tx,
        ));
        rx.await.expect("run_flowgraph crashed").unwrap();
        let handle = FlowgraphHandle::new(fg_inbox);
        self.flowgraphs.lock().unwrap().insert(handle.clone());
        (TaskHandle::new(task), handle)
    }

    /// Start a [`Flowgraph`] on the [`Runtime`]
    ///
    /// Blocks until the flowgraph is constructed and running.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn start_sync(
        &self,
        fg: Flowgraph,
    ) -> (TaskHandle<Result<Flowgraph, Error>>, FlowgraphHandle) {
        block_on(self.start(fg))
    }

    /// Start a [`Flowgraph`] on the [`Runtime`] and block until it terminates.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run(&self, fg: Flowgraph) -> Result<Flowgraph, Error> {
        let (handle, _) = block_on(self.start(fg));
        block_on(handle)
    }

    /// Start a [`Flowgraph`] on the [`Runtime`] and await its termination.
    pub async fn run_async(&self, fg: Flowgraph) -> Result<Flowgraph, Error> {
        let (handle, _) = self.start(fg).await;
        handle.await
    }

    /// Get the [`Scheduler`] that is associated with the [`Runtime`].
    pub fn scheduler(&self) -> S {
        self.scheduler.clone()
    }

    /// Get the [`RuntimeHandle`]
    pub fn handle(&self) -> RuntimeHandle {
        RuntimeHandle {
            flowgraphs: self.flowgraphs.clone(),
            scheduler: Arc::new(self.scheduler.clone()),
        }
    }
}

#[async_trait]
trait Spawn {
    async fn start(&self, fg: Flowgraph) -> Result<FlowgraphHandle, Error>;
}

#[async_trait]
impl<S: Scheduler + Sync + 'static> Spawn for S {
    async fn start(&self, fg: Flowgraph) -> Result<FlowgraphHandle, Error> {
        use crate::runtime::runtime::run_flowgraph;
        use crate::runtime::FlowgraphMessage;
        use futures::channel::mpsc::channel;
        use futures::channel::oneshot;

        let queue_size = config::config().queue_size;
        let (fg_inbox, fg_inbox_rx) = channel::<FlowgraphMessage>(queue_size);

        let (tx, rx) = oneshot::channel::<Result<(), Error>>();
        self.spawn(run_flowgraph(
            fg,
            self.clone(),
            fg_inbox.clone(),
            fg_inbox_rx,
            tx,
        ))
        .detach();
        rx.await.or(Err(Error::RuntimeError(
            "run_flowgraph crashed".to_string(),
        )))??;
        Ok(FlowgraphHandle::new(fg_inbox))
    }
}

/// Runtime handle added as state to web handlers
#[derive(Clone)]
pub struct RuntimeHandle {
    scheduler: Arc<dyn Spawn + Send + Sync + 'static>,
    flowgraphs: Arc<Mutex<Slab<FlowgraphHandle>>>,
}

impl fmt::Debug for RuntimeHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RuntimeHandle")
            .field("flowgraphs", &self.flowgraphs)
            .finish()
    }
}

impl PartialEq for RuntimeHandle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.scheduler, &other.scheduler)
    }
}

impl RuntimeHandle {
    /// Start a [`Flowgraph`] on the runtime
    pub async fn start(&self, fg: Flowgraph) -> Result<FlowgraphHandle, Error> {
        let handle = self.scheduler.start(fg).await?;

        self.add_flowgraph(handle.clone());
        Ok(handle)
    }

    /// Add a [`FlowgraphHandle`] to make it available to web handlers
    fn add_flowgraph(&self, handle: FlowgraphHandle) -> usize {
        let mut v = self.flowgraphs.lock().unwrap();
        v.insert(handle)
    }

    /// Get handle to a running flowgraph
    pub fn get_flowgraph(&self, id: usize) -> Option<FlowgraphHandle> {
        self.flowgraphs.lock().unwrap().get(id).cloned()
    }

    /// Get list of flowgraph IDs
    pub fn get_flowgraphs(&self) -> Vec<usize> {
        self.flowgraphs
            .lock()
            .unwrap()
            .iter()
            .map(|x| x.0)
            .collect()
    }
}

pub(crate) async fn run_flowgraph<S: Scheduler>(
    mut fg: Flowgraph,
    scheduler: S,
    mut main_channel: Sender<FlowgraphMessage>,
    mut main_rx: Receiver<FlowgraphMessage>,
    initialized: oneshot::Sender<Result<(), Error>>,
) -> Result<Flowgraph, Error> {
    debug!("in run_flowgraph");
    let mut topology = fg.topology.take().ok_or(Error::RuntimeError(
        "Flowgraph has no topology set".to_string(),
    ))?;
    if let Err(e) = topology.validate() {
        initialized.send(Err(e.clone())).unwrap();
        return Err(e);
    }

    let mut inboxes = scheduler.run_topology(&mut topology, &main_channel);

    debug!("connect stream io");
    // connect stream IO
    for ((src, src_port, buffer_builder), v) in topology.stream_edges.iter() {
        debug_assert!(!v.is_empty());

        let src_inbox = inboxes[*src].as_ref().unwrap().clone();
        let mut writer = buffer_builder.build(src_inbox, *src_port);

        for (dst, dst_port) in v.iter() {
            let dst_inbox = inboxes[*dst].as_ref().unwrap().clone();

            inboxes[*dst]
                .as_mut()
                .unwrap()
                .send(BlockMessage::StreamInputInit {
                    dst_port: *dst_port,
                    reader: writer.add_reader(dst_inbox, *dst_port),
                })
                .await
                .or(Err(Error::RuntimeError(
                    "Could not connect stream input".to_string(),
                )))?;
        }

        inboxes[*src]
            .as_mut()
            .unwrap()
            .send(BlockMessage::StreamOutputInit {
                src_port: *src_port,
                writer,
            })
            .await
            .or(Err(Error::RuntimeError(
                "Could not connect stream output".to_string(),
            )))?;
    }

    debug!("connect message io");
    // connect message IO
    for (src, src_port, dst, dst_port) in topology.message_edges.iter() {
        let dst_box = inboxes[*dst].as_ref().unwrap().clone();
        inboxes[*src]
            .as_mut()
            .unwrap()
            .send(BlockMessage::MessageOutputConnect {
                src_port: *src_port,
                dst_port: *dst_port,
                dst_inbox: dst_box,
            })
            .await
            .unwrap();
    }

    debug!("init blocks");
    // init blocks
    let mut active_blocks = 0u32;
    for (_, opt) in inboxes.iter_mut() {
        if let Some(ref mut chan) = opt {
            chan.send(BlockMessage::Initialize).await.unwrap();
            active_blocks += 1;
        }
    }

    debug!("wait for blocks init");
    // wait until all blocks are initialized
    let mut i = active_blocks;
    let mut queue = Vec::new();
    let mut block_error = false;
    loop {
        if i == 0 {
            break;
        }

        let m = main_rx.next().await.unwrap();
        match m {
            FlowgraphMessage::Initialized => i -= 1,
            FlowgraphMessage::BlockError { block_id, block } => {
                *topology.blocks.get_mut(block_id).unwrap() = Some(block);
                inboxes[block_id] = None;
                i -= 1;
                active_blocks -= 1;
                block_error = true;
            }
            x => {
                debug!(
                    "queueing unhandled message received during initialization {:?}",
                    &x
                );
                queue.push(x);
            }
        }
    }

    debug!("running blocks");
    for (_, opt) in inboxes.iter_mut() {
        if let Some(ref mut chan) = opt {
            if chan.send(BlockMessage::Notify).await.is_err() {
                debug!("runtime wanted to start block that already terminated");
            }
        }
    }

    for m in queue.into_iter() {
        main_channel
            .try_send(m)
            .expect("main inbox exceeded capacity during startup");
    }

    initialized
        .send(Ok(()))
        .expect("failed to signal flowgraph startup complete.");

    if block_error {
        main_channel
            .try_send(FlowgraphMessage::Terminate)
            .expect("main inbox exceeded capacity during startup");
    }

    let mut terminated = false;

    // main loop
    loop {
        if active_blocks == 0 {
            break;
        }

        let m = main_rx.next().await.unwrap();
        match m {
            FlowgraphMessage::BlockCall {
                block_id,
                port_id,
                data,
                tx,
            } => {
                if let Some(inbox) = inboxes[block_id].as_mut() {
                    if inbox
                        .send(BlockMessage::Call { port_id, data })
                        .await
                        .is_ok()
                    {
                        let _ = tx.send(Ok(()));
                    } else {
                        let _ = tx.send(Err(Error::BlockTerminated));
                    }
                } else {
                    let _ = tx.send(Err(Error::InvalidBlock(block_id)));
                }
            }
            FlowgraphMessage::BlockCallback {
                block_id,
                port_id,
                data,
                tx,
            } => {
                let (block_tx, block_rx) = oneshot::channel::<Result<Pmt, Error>>();
                if let Some(Some(inbox)) = inboxes.get_mut(block_id) {
                    if inbox
                        .send(BlockMessage::Callback {
                            port_id,
                            data,
                            tx: block_tx,
                        })
                        .await
                        .is_ok()
                    {
                        match block_rx.await? {
                            Ok(p) => tx.send(Ok(p)).ok(),
                            Err(e) => tx.send(Err(Error::HandlerError(e.to_string()))).ok(),
                        };
                    } else {
                        let _ = tx.send(Err(Error::BlockTerminated));
                    }
                } else {
                    let _ = tx.send(Err(Error::InvalidBlock(block_id)));
                }
            }
            FlowgraphMessage::BlockDone { block_id, block } => {
                *topology.blocks.get_mut(block_id).unwrap() = Some(block);
                inboxes[block_id] = None;
                active_blocks -= 1;
            }
            FlowgraphMessage::BlockError { block_id, block } => {
                *topology.blocks.get_mut(block_id).unwrap() = Some(block);
                inboxes[block_id] = None;
                block_error = true;
                active_blocks -= 1;
                let _ = main_channel.send(FlowgraphMessage::Terminate).await;
            }
            FlowgraphMessage::BlockDescription { block_id, tx } => {
                if let Some(Some(ref mut b)) = inboxes.get_mut(block_id) {
                    let (b_tx, rx) = oneshot::channel::<BlockDescription>();
                    if b.send(BlockMessage::BlockDescription { tx: b_tx })
                        .await
                        .is_ok()
                    {
                        if let Ok(b) = rx.await {
                            let _ = tx.send(Ok(b));
                        } else {
                            let _ = tx.send(Err(Error::RuntimeError(format!(
                                "Block {} terminated or crashed",
                                block_id
                            ))));
                        }
                    } else {
                        let _ = tx.send(Err(Error::BlockTerminated));
                    }
                } else {
                    let _ = tx.send(Err(Error::InvalidBlock(block_id)));
                }
            }
            FlowgraphMessage::FlowgraphDescription { tx } => {
                let mut blocks = Vec::new();
                let ids: Vec<usize> = topology.blocks.iter().map(|x| x.0).collect();
                for id in ids {
                    let (b_tx, rx) = oneshot::channel::<BlockDescription>();
                    if let Some(Some(inbox)) = inboxes.get_mut(id) {
                        inbox
                            .send(BlockMessage::BlockDescription { tx: b_tx })
                            .await
                            .unwrap();
                        blocks.push(rx.await.unwrap());
                    }
                }

                let stream_edges = topology
                    .stream_edges
                    .iter()
                    .flat_map(|x| x.1.iter().map(|y| (x.0 .0, x.0 .1, y.0, y.1)))
                    .collect();
                let message_edges = topology.message_edges.clone();

                if tx
                    .send(FlowgraphDescription {
                        blocks,
                        stream_edges,
                        message_edges,
                    })
                    .is_err()
                {
                    error!("Failed to send flowgraph description. Receiver may have disconnected.");
                }
            }
            FlowgraphMessage::Terminate => {
                if !terminated {
                    for (_, opt) in inboxes.iter_mut() {
                        if let Some(ref mut chan) = opt {
                            if chan.send(BlockMessage::Terminate).await.is_err() {
                                debug!(
                                    "runtime tried to terminate block that was already terminated"
                                );
                            }
                        }
                    }
                    terminated = true;
                }
            }
            _ => warn!("main loop received unhandled message"),
        }
    }

    fg.topology = Some(topology);
    if block_error {
        return Err(Error::RuntimeError("A block raised an error".to_string()));
    }

    Ok(fg)
}
