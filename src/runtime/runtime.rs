use async_lock::Mutex;
#[cfg(all(not(target_arch = "wasm32"), feature = "ctrl_port"))]
use axum::Router;
use futures::prelude::*;
use std::collections::BTreeMap;
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
use crate::runtime::flowgraph::run_flowgraph;
use crate::runtime::flowgraph_handle::RunningFlowgraphControl;
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
    flowgraphs: Arc<Mutex<FlowgraphRegistry>>,
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
        self.flowgraphs.lock().await.insert(running.handle());
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

        let flowgraphs = Arc::new(Mutex::new(FlowgraphRegistry::default()));
        let handle = RuntimeHandle {
            scheduler: scheduler.clone(),
            flowgraphs: flowgraphs.clone(),
        };

        Runtime {
            scheduler: scheduler.clone(),
            flowgraphs,
            _control_port: ControlPort::new(handle, scheduler, routes),
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(feature = "ctrl_port")))]
impl<S: Scheduler> Runtime<S> {
    /// Construct a [`Runtime`] with a custom [`Scheduler`].
    pub fn with_scheduler(scheduler: S) -> Self {
        runtime::init();

        let flowgraphs = Arc::new(Mutex::new(FlowgraphRegistry::default()));
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

        let flowgraphs = Arc::new(Mutex::new(FlowgraphRegistry::default()));
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
    flowgraphs: Arc<Mutex<FlowgraphRegistry>>,
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
        self.flowgraphs.lock().await.insert(handle)
    }

    /// Get the control handle for a flowgraph by stable flowgraph id.
    ///
    /// Terminated flowgraphs are pruned from the registry and return `None`.
    /// The ids of other running flowgraphs are not changed by pruning.
    pub async fn get_flowgraph(&self, id: FlowgraphId) -> Option<FlowgraphHandle> {
        self.flowgraphs.lock().await.get(id)
    }

    /// Get the stable ids of running flowgraphs known to this runtime handle.
    pub async fn get_flowgraphs(&self) -> Vec<FlowgraphId> {
        self.flowgraphs.lock().await.running_ids()
    }
}

#[derive(Debug, Default)]
struct FlowgraphRegistry {
    flowgraphs: BTreeMap<FlowgraphId, FlowgraphHandle>,
}

impl FlowgraphRegistry {
    fn insert(&mut self, handle: FlowgraphHandle) -> FlowgraphId {
        let id = handle.id();
        self.flowgraphs.insert(id, handle);
        id
    }

    fn get(&mut self, id: FlowgraphId) -> Option<FlowgraphHandle> {
        match self.flowgraphs.get(&id) {
            Some(handle) if handle.is_terminated() => {
                self.flowgraphs.remove(&id);
                None
            }
            Some(handle) => Some(handle.clone()),
            None => None,
        }
    }

    fn running_ids(&mut self) -> Vec<FlowgraphId> {
        self.prune_terminated();
        self.flowgraphs.keys().copied().collect()
    }

    fn prune_terminated(&mut self) {
        self.flowgraphs.retain(|_, handle| !handle.is_terminated());
    }
}

async fn start_flowgraph<S: Scheduler>(
    scheduler: S,
    fg: Flowgraph,
) -> Result<RunningFlowgraph, Error> {
    let id = fg.id();
    let queue_size = config::config().queue_size;
    let (fg_inbox, fg_inbox_rx) = channel::<FlowgraphMessage>(queue_size);

    let (tx, rx) = oneshot::channel::<Result<(), Error>>();
    let (control_tx, control_rx) = oneshot::channel::<RunningFlowgraphControl>();
    let scheduler_clone = scheduler.clone();
    let task = scheduler.spawn(run_flowgraph(
        fg,
        scheduler_clone,
        fg_inbox.clone(),
        fg_inbox_rx,
        tx,
        control_tx,
    ));

    rx.await
        .map_err(|_| Error::RuntimeError("run_flowgraph panicked".to_string()))??;
    let control = control_rx.await.map_err(|_| {
        Error::RuntimeError("run_flowgraph did not publish control endpoints".to_string())
    })?;

    let handle = FlowgraphHandle::new(id, fg_inbox, control);
    Ok(RunningFlowgraph::new(handle, FlowgraphTask::new(task)))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::blocks::MessageSourceBuilder;
    use crate::runtime::Pmt;
    use std::time::Duration;

    fn message_source_flowgraph(n_messages: Option<usize>) -> Flowgraph {
        let mut fg = Flowgraph::new();
        let builder = MessageSourceBuilder::new(Pmt::Null, Duration::from_millis(1));
        let builder = match n_messages {
            Some(n) => builder.n_messages(n),
            None => builder,
        };
        fg.add(builder.build()).unwrap();
        fg
    }

    #[test]
    fn terminated_flowgraphs_are_not_returned_by_registry() {
        let scheduler = DefaultScheduler::default();
        let handle = RuntimeHandle {
            scheduler,
            flowgraphs: Arc::new(Mutex::new(FlowgraphRegistry::default())),
        };

        runtime::block_on(async {
            let running = handle
                .start(message_source_flowgraph(Some(1)))
                .await
                .unwrap();
            let id = running.id();

            assert_eq!(handle.get_flowgraphs().await, vec![id]);

            running.wait_async().await.unwrap();

            assert!(handle.get_flowgraph(id).await.is_none());
            assert!(handle.get_flowgraphs().await.is_empty());
        });
    }

    #[test]
    fn terminated_flowgraph_cleanup_does_not_change_other_ids() {
        let scheduler = DefaultScheduler::default();
        let handle = RuntimeHandle {
            scheduler,
            flowgraphs: Arc::new(Mutex::new(FlowgraphRegistry::default())),
        };

        runtime::block_on(async {
            let first = handle
                .start(message_source_flowgraph(Some(1)))
                .await
                .unwrap();
            let first_id = first.id();
            let second = handle.start(message_source_flowgraph(None)).await.unwrap();
            let second_id = second.id();

            first.wait_async().await.unwrap();

            assert!(handle.get_flowgraph(first_id).await.is_none());
            assert_eq!(
                handle.get_flowgraph(second_id).await.unwrap().id(),
                second_id
            );
            assert_eq!(handle.get_flowgraphs().await, vec![second_id]);

            let third = handle.start(message_source_flowgraph(None)).await.unwrap();
            let third_id = third.id();

            assert_ne!(third_id, first_id);
            assert_ne!(third_id, second_id);
            assert_eq!(
                handle.get_flowgraph(second_id).await.unwrap().id(),
                second_id
            );
            assert_eq!(handle.get_flowgraphs().await, vec![second_id, third_id]);

            second.stop_and_wait().await.unwrap();
            third.stop_and_wait().await.unwrap();
        });
    }
}
