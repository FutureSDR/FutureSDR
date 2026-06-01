use async_lock::Mutex;
#[cfg(all(not(target_arch = "wasm32"), feature = "ctrl_port"))]
use axum::Router;
use futures::prelude::*;
use std::fmt;
use std::sync::Arc;

use crate::runtime;
#[cfg(all(not(target_arch = "wasm32"), feature = "ctrl_port"))]
use crate::runtime::ControlPort;
use crate::runtime::Error;
use crate::runtime::Flowgraph;
use crate::runtime::FlowgraphHandle;
use crate::runtime::FlowgraphId;
use crate::runtime::FlowgraphMessage;
use crate::runtime::FlowgraphTask;
use crate::runtime::RunningFlowgraph;
use crate::runtime::TerminatedFlowgraph;
use crate::runtime::channel::mpsc::channel;
use crate::runtime::channel::oneshot;
use crate::runtime::config;
use crate::runtime::scheduler::Scheduler;
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::scheduler::SmolScheduler;
use crate::runtime::scheduler::Task;
#[cfg(target_arch = "wasm32")]
use crate::runtime::scheduler::WasmMainScheduler;

#[cfg(not(target_arch = "wasm32"))]
/// Default scheduler used by [`Runtime`] and [`RuntimeHandle`] on native targets.
pub type DefaultScheduler = SmolScheduler;

#[cfg(target_arch = "wasm32")]
/// Default scheduler used by [`Runtime`] and [`RuntimeHandle`] on WASM targets.
pub type DefaultScheduler = WasmMainScheduler;

/// Executor and control-plane owner for [`Flowgraph`]s and async tasks.
///
/// A [`Runtime`] owns a scheduler, starts flowgraphs, and provides a control
/// port on native targets when the `ctrl_port` feature is enabled. It is generic
/// over the scheduler implementation, but most applications should use
/// [`Runtime::new`] with the default scheduler.
///
/// Use [`Runtime::run`] or [`Runtime::run_async`] when the caller should wait
/// until a flowgraph finishes. Use [`Runtime::start`] or
/// [`Runtime::start_async`] when the caller needs a [`RunningFlowgraph`] handle
/// for live message calls, descriptions, or shutdown.
pub struct Runtime<S = DefaultScheduler> {
    scheduler: S,
    flowgraphs: Arc<Mutex<Vec<FlowgraphHandle>>>,
    #[cfg(all(not(target_arch = "wasm32"), feature = "ctrl_port"))]
    _control_port: ControlPort<S>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Runtime<DefaultScheduler> {
    /// Construct a new [`Runtime`] using [`DefaultScheduler::default()`].
    ///
    /// On native targets this also initializes logging and, with the `ctrl_port`
    /// feature, starts the integrated control-port server when the runtime
    /// configuration enables it.
    pub fn new() -> Self {
        Self::with_scheduler(DefaultScheduler::default())
    }

    /// Construct a runtime with additional routes for the integrated web server.
    ///
    /// The routes are merged into the native control-port server. Use this for
    /// application-specific HTTP APIs or UI assets that should be served by the
    /// same process.
    #[cfg(feature = "ctrl_port")]
    pub fn with_custom_routes(routes: Router) -> Self {
        Self::with_config(DefaultScheduler::default(), routes)
    }
}

impl<S> Drop for Runtime<S> {
    fn drop(&mut self) {
        debug!("Runtime dropped");
    }
}

#[cfg(target_arch = "wasm32")]
impl Runtime<DefaultScheduler> {
    /// Construct a runtime using the default main-thread WASM scheduler.
    ///
    /// WASM runtimes do not start a native control-port server. Use
    /// [`WasmScheduler`](crate::runtime::scheduler::WasmScheduler) explicitly
    /// with [`Runtime::with_scheduler`] when worker-backed execution is desired.
    pub fn new() -> Self {
        Self::with_scheduler(DefaultScheduler::default())
    }
}

impl Default for Runtime<DefaultScheduler> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Scheduler> Runtime<S> {
    /// Spawn an async task on the runtime scheduler and return its task handle.
    ///
    /// The task is unrelated to any particular flowgraph. Dropping the returned
    /// task cancels or detaches according to the underlying scheduler task type.
    pub fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        self.scheduler.spawn(future)
    }

    /// Spawn an async task on the runtime scheduler and detach it immediately.
    pub fn spawn_background<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) {
        self.scheduler.spawn(future).detach();
    }

    /// Start a [`Flowgraph`] on the [`Runtime`] and await initialization.
    ///
    /// Returns once the flowgraph is initialized and running. The returned
    /// [`RunningFlowgraph`] can be used to send messages, stop the graph, or
    /// wait for completion.
    pub async fn start_async(&self, fg: Flowgraph) -> Result<RunningFlowgraph, Error> {
        let running = start_flowgraph(self.scheduler.clone(), fg).await?;
        self.flowgraphs.lock().await.push(running.handle());
        Ok(running)
    }

    /// Start a [`Flowgraph`] on the [`Runtime`] and await its termination.
    ///
    /// This consumes the input flowgraph, runs it until every block finishes or
    /// an error stops execution, and returns a [`TerminatedFlowgraph`] so final
    /// block state can be inspected.
    pub async fn run_async(&self, fg: Flowgraph) -> Result<TerminatedFlowgraph, Error> {
        self.start_async(fg).await?.wait_async().await
    }

    /// Get the [`Scheduler`] that is associated with the [`Runtime`].
    pub fn scheduler(&self) -> &S {
        &self.scheduler
    }

    /// Create a clonable [`RuntimeHandle`] for starting and querying flowgraphs.
    ///
    /// Handles share the same scheduler and control-plane registry as this
    /// runtime. They are intended for web handlers, callbacks, and other async
    /// tasks that cannot borrow the runtime directly.
    pub fn handle(&self) -> RuntimeHandle<S> {
        RuntimeHandle {
            scheduler: self.scheduler.clone(),
            flowgraphs: self.flowgraphs.clone(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<S: Scheduler> Runtime<S> {
    /// Start a [`Flowgraph`] on the [`Runtime`].
    ///
    /// Blocks until the flowgraph is initialized and running.
    pub fn start(&self, fg: Flowgraph) -> Result<RunningFlowgraph, Error> {
        runtime::block_on(self.start_async(fg))
    }

    /// Start a [`Flowgraph`] on the [`Runtime`] and block until it terminates.
    ///
    /// This is the synchronous counterpart of [`Runtime::run_async`].
    pub fn run(&self, fg: Flowgraph) -> Result<TerminatedFlowgraph, Error> {
        let running = runtime::block_on(self.start_async(fg))?;
        running.wait()
    }
}

#[cfg(all(not(target_arch = "wasm32"), feature = "ctrl_port"))]
impl<S: Scheduler + Sync> Runtime<S> {
    /// Construct a [`Runtime`] with a custom [`Scheduler`].
    ///
    /// This uses the normal native control-port routes without adding
    /// application-specific routes.
    pub fn with_scheduler(scheduler: S) -> Self {
        Self::with_config(scheduler, Router::new())
    }

    /// Construct a runtime with a custom scheduler and web server routes.
    pub fn with_config(scheduler: S, routes: Router) -> Self {
        runtime::init();

        let flowgraphs = Arc::new(Mutex::new(Vec::new()));
        let handle = RuntimeHandle {
            scheduler: scheduler.clone(),
            flowgraphs: flowgraphs.clone(),
        };

        Runtime {
            scheduler,
            flowgraphs,
            _control_port: ControlPort::new(handle, routes),
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(feature = "ctrl_port")))]
impl<S: Scheduler> Runtime<S> {
    /// Construct a [`Runtime`] with a custom [`Scheduler`].
    pub fn with_scheduler(scheduler: S) -> Self {
        runtime::init();

        let flowgraphs = Arc::new(Mutex::new(Vec::new()));
        Runtime {
            scheduler,
            flowgraphs,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl<S: Scheduler> Runtime<S> {
    /// Construct a [`Runtime`] with a custom [`Scheduler`].
    pub fn with_scheduler(scheduler: S) -> Self {
        runtime::init();

        let flowgraphs = Arc::new(Mutex::new(Vec::new()));
        Runtime {
            scheduler,
            flowgraphs,
        }
    }
}

/// Clonable runtime control handle used by web handlers and external control code.
///
/// A `RuntimeHandle` can start new flowgraphs on the same scheduler as the
/// owning [`Runtime`] and look up flowgraphs that have been registered with the
/// control plane.
pub struct RuntimeHandle<S = DefaultScheduler> {
    scheduler: S,
    flowgraphs: Arc<Mutex<Vec<FlowgraphHandle>>>,
}

impl<S: Clone> Clone for RuntimeHandle<S> {
    fn clone(&self) -> Self {
        Self {
            scheduler: self.scheduler.clone(),
            flowgraphs: self.flowgraphs.clone(),
        }
    }
}

impl<S> fmt::Debug for RuntimeHandle<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RuntimeHandle")
            .field("flowgraphs", &self.flowgraphs)
            .finish()
    }
}

impl<S> PartialEq for RuntimeHandle<S> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.flowgraphs, &other.flowgraphs)
    }
}

impl<S: Scheduler> RuntimeHandle<S> {
    /// Start a [`Flowgraph`] on the runtime and register it with the control plane.
    ///
    /// This has the same startup semantics as [`Runtime::start_async`]. The
    /// returned flowgraph is available through [`RuntimeHandle::get_flowgraph`]
    /// and the native control-port API until it terminates.
    pub async fn start(&self, fg: Flowgraph) -> Result<RunningFlowgraph, Error> {
        let running = start_flowgraph(self.scheduler.clone(), fg).await?;
        self.add_flowgraph(running.handle()).await;
        Ok(running)
    }

    /// Add a [`FlowgraphHandle`] to make it available to web handlers.
    async fn add_flowgraph(&self, handle: FlowgraphHandle) -> FlowgraphId {
        let mut v = self.flowgraphs.lock().await;
        let l = v.len();
        v.push(handle);
        FlowgraphId(l)
    }

    /// Get the control handle for a flowgraph by runtime registry id.
    ///
    /// The id is the position assigned when the flowgraph was registered with
    /// the runtime handle. The returned handle may still fail later if the
    /// flowgraph has already terminated.
    pub async fn get_flowgraph(&self, id: FlowgraphId) -> Option<FlowgraphHandle> {
        self.flowgraphs.lock().await.get(id.0).cloned()
    }

    /// Get the ids of flowgraphs known to this runtime handle.
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

async fn start_flowgraph<S: Scheduler>(
    scheduler: S,
    fg: Flowgraph,
) -> Result<RunningFlowgraph, Error> {
    let queue_size = config::config().queue_size;
    let (fg_inbox, fg_inbox_rx) = channel::<FlowgraphMessage>(queue_size);

    let (tx, rx) = oneshot::channel::<Result<(), Error>>();
    let scheduler_clone = scheduler.clone();
    let task =
        scheduler.spawn(fg.run_flowgraph(scheduler_clone, fg_inbox.clone(), fg_inbox_rx, tx));

    rx.await
        .map_err(|_| Error::RuntimeError("run_flowgraph panicked".to_string()))??;

    let handle = FlowgraphHandle::new(fg_inbox);
    Ok(RunningFlowgraph::new(handle, FlowgraphTask::new(task)))
}
