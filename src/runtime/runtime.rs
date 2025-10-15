#[cfg(not(target_arch = "wasm32"))]
use async_io::block_on;
use async_lock::Mutex;
#[cfg(not(target_arch = "wasm32"))]
use axum::Router;
use futures::FutureExt;
use futures::channel::mpsc::Receiver;
use futures::channel::mpsc::Sender;
use futures::channel::mpsc::channel;
use futures::channel::oneshot;
use futures::prelude::*;
use std::fmt;
use std::pin::Pin;
use std::sync::Arc;
use std::task;
use std::task::Poll;

use crate::runtime;
use crate::runtime::BlockDescription;
use crate::runtime::BlockMessage;
use crate::runtime::ControlPort;
use crate::runtime::Error;
use crate::runtime::Flowgraph;
use crate::runtime::FlowgraphDescription;
use crate::runtime::FlowgraphHandle;
use crate::runtime::FlowgraphId;
use crate::runtime::FlowgraphMessage;
use crate::runtime::Pmt;
use crate::runtime::config;
use crate::runtime::scheduler::Scheduler;
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::scheduler::SmolScheduler;
use crate::runtime::scheduler::Task;
#[cfg(target_arch = "wasm32")]
use crate::runtime::scheduler::WasmScheduler;

pub struct TaskHandle<'a, T> {
    task: std::mem::ManuallyDrop<Task<T>>,
    _p: std::marker::PhantomData<&'a ()>,
}

#[cfg(not(target_arch = "wasm32"))]
impl<T> Drop for TaskHandle<'_, T> {
    fn drop(&mut self) {
        // SAFETY: We take ownership of the `Task<T>`
        // and then call `detach`. Because `task` is in a ManuallyDrop,
        // the compiler won’t automatically drop it afterwards.
        let task = unsafe { std::ptr::read(&*self.task) };
        task.detach();
    }
}

impl<T> TaskHandle<'_, T> {
    fn new(task: Task<T>) -> Self {
        TaskHandle {
            task: std::mem::ManuallyDrop::new(task),
            _p: std::marker::PhantomData,
        }
    }
}

impl<T> std::future::Future for TaskHandle<'_, T> {
    type Output = T;
    fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        self.task.poll_unpin(cx)
    }
}

/// The [Runtime] runs [Flowgraph]s and async tasks
///
/// [Runtime]s are generic over the scheduler used to run the [Flowgraph].
pub struct Runtime<'a, S> {
    scheduler: S,
    flowgraphs: Arc<Mutex<Vec<FlowgraphHandle>>>,
    _control_port: ControlPort,
    _p: std::marker::PhantomData<&'a ()>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Runtime<'_, SmolScheduler> {
    /// Constructs a new [Runtime] using [SmolScheduler::default()] for the [Scheduler].
    pub fn new() -> Self {
        Self::with_custom_routes(Router::new())
    }

    /// Set custom routes for the integrated webserver
    pub fn with_custom_routes(routes: Router) -> Self {
        runtime::init();
        let scheduler = SmolScheduler::default();
        let flowgraphs = Arc::new(Mutex::new(Vec::new()));
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
        let flowgraphs = Arc::new(Mutex::new(Vec::new()));
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

    /// Create runtime with given scheduler and custom routes for the integrated webserver
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_config(scheduler: S, routes: Router) -> Self {
        runtime::init();

        let flowgraphs = Arc::new(Mutex::new(Vec::new()));
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

    /// Spawn an async task on the runtime
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

    /// Spawn async task on the runtime, detaching the handle
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
    ) -> Result<(TaskHandle<'b, Result<Flowgraph, Error>>, FlowgraphHandle), Error>
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
        rx.await
            .map_err(|_| Error::RuntimeError("run_flowgraph panicked".to_string()))??;
        let handle = FlowgraphHandle::new(fg_inbox);
        self.flowgraphs
            .try_lock()
            .ok_or(Error::LockError)?
            .push(handle.clone());
        Ok((TaskHandle::new(task), handle))
    }

    /// Start a [`Flowgraph`] on the [`Runtime`]
    ///
    /// Blocks until the flowgraph is constructed and running.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn start_sync(
        &self,
        fg: Flowgraph,
    ) -> Result<(TaskHandle<'_, Result<Flowgraph, Error>>, FlowgraphHandle), Error> {
        block_on(self.start(fg))
    }

    /// Start a [`Flowgraph`] on the [`Runtime`] and block until it terminates.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run(&self, fg: Flowgraph) -> Result<Flowgraph, Error> {
        let (handle, _) = block_on(self.start(fg))?;
        block_on(handle)
    }

    /// Start a [`Flowgraph`] on the [`Runtime`] and await its termination.
    pub async fn run_async(&self, fg: Flowgraph) -> Result<Flowgraph, Error> {
        let (handle, _) = self.start(fg).await?;
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
        use crate::runtime::FlowgraphMessage;
        use crate::runtime::runtime::run_flowgraph;
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
    flowgraphs: Arc<Mutex<Vec<FlowgraphHandle>>>,
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

        self.add_flowgraph(handle.clone()).await;
        Ok(handle)
    }

    /// Add a [`FlowgraphHandle`] to make it available to web handlers
    async fn add_flowgraph(&self, handle: FlowgraphHandle) -> FlowgraphId {
        let mut v = self.flowgraphs.lock().await;
        let l = v.len();
        v.push(handle);
        FlowgraphId(l)
    }

    /// Get handle to a running flowgraph
    pub async fn get_flowgraph(&self, id: FlowgraphId) -> Option<FlowgraphHandle> {
        self.flowgraphs.lock().await.get(id.0).cloned()
    }

    /// Get list of flowgraph IDs
    pub async fn get_flowgraphs(&self) -> Vec<FlowgraphId> {
        self.flowgraphs
            .lock()
            .await
            .iter()
            .enumerate()
            .map(|x| FlowgraphId(x.0))
            .collect()
    }
}

pub(crate) async fn run_flowgraph<S: Scheduler>(
    fg: Flowgraph,
    scheduler: S,
    mut main_channel: Sender<FlowgraphMessage>,
    mut main_rx: Receiver<FlowgraphMessage>,
    initialized: oneshot::Sender<Result<(), Error>>,
) -> Result<Flowgraph, Error> {
    debug!("in run_flowgraph");

    let mut inboxes = vec![];
    for b in fg.blocks.iter() {
        inboxes.push(b.lock().await.inbox())
    }
    let mut ids = vec![];
    for b in fg.blocks.iter() {
        ids.push(b.lock().await.id());
    }

    scheduler.run_flowgraph(fg.blocks.clone(), &main_channel);

    debug!("init blocks");
    // init blocks
    let mut active_blocks = 0u32;
    for inbox in inboxes.iter_mut() {
        inbox.send(BlockMessage::Initialize).await?;
        active_blocks += 1;
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

        let m = main_rx.next().await.ok_or_else(|| {
            Error::RuntimeError("no reply from blocks during init phase".to_string())
        })?;
        match m {
            FlowgraphMessage::Initialized => i -= 1,
            FlowgraphMessage::BlockError { .. } => {
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
    for inbox in inboxes.iter_mut() {
        if inbox.send(BlockMessage::Notify).await.is_err() {
            debug!("runtime wanted to start block that already terminated");
        }
    }

    for m in queue.into_iter() {
        main_channel.try_send(m)?;
    }

    initialized
        .send(Ok(()))
        .map_err(|_| Error::RuntimeError("main thread panic during flowgraph init".to_string()))?;

    if block_error {
        main_channel.try_send(FlowgraphMessage::Terminate)?;
    }

    let mut terminated = false;

    // main loop
    loop {
        if active_blocks == 0 {
            break;
        }

        let m = main_rx.next().await.ok_or_else(|| {
            Error::RuntimeError("all senders to flowgraph inbox dropped".to_string())
        })?;
        match m {
            FlowgraphMessage::BlockCall {
                block_id,
                port_id,
                data,
                tx,
            } => {
                if let Some(inbox) = inboxes.get_mut(block_id.0) {
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
                if let Some(inbox) = inboxes.get_mut(block_id.0) {
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
            FlowgraphMessage::BlockDone { .. } => {
                active_blocks -= 1;
            }
            FlowgraphMessage::BlockError { .. } => {
                block_error = true;
                active_blocks -= 1;
                let _ = main_channel.send(FlowgraphMessage::Terminate).await;
            }
            FlowgraphMessage::BlockDescription { block_id, tx } => {
                if let Some(ref mut b) = inboxes.get_mut(block_id.0) {
                    let (b_tx, rx) = oneshot::channel::<BlockDescription>();
                    if b.send(BlockMessage::BlockDescription { tx: b_tx })
                        .await
                        .is_ok()
                    {
                        if let Ok(b) = rx.await {
                            let _ = tx.send(Ok(b));
                        } else {
                            let _ = tx.send(Err(Error::RuntimeError(format!(
                                "Block {block_id:?} terminated or crashed"
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
                for id in ids.iter() {
                    let (b_tx, rx) = oneshot::channel::<BlockDescription>();
                    if let Some(inbox) = inboxes.get_mut(id.0) {
                        if inbox
                            .send(BlockMessage::BlockDescription { tx: b_tx })
                            .await
                            .is_ok()
                        {
                            blocks.push(rx.await?);
                        }
                    }
                }

                let stream_edges = fg.stream_edges.clone();
                let message_edges = fg.message_edges.clone();

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
                    for inbox in inboxes.iter_mut() {
                        if inbox.send(BlockMessage::Terminate).await.is_err() {
                            debug!("runtime tried to terminate block that was already terminated");
                        }
                    }
                    terminated = true;
                }
            }
            _ => warn!("main loop received unhandled message"),
        }
    }

    if block_error {
        return Err(Error::RuntimeError("A block raised an error".to_string()));
    }

    Ok(fg)
}
