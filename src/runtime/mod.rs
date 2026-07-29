//! Build, run, and control SDR flowgraphs.
//!
//! This module is the main application-facing runtime surface. It contains the
//! types used to construct [`Flowgraph`]s, start them on a [`Runtime`],
//! interact with running graphs through [`RunningFlowgraph`] and
//! [`FlowgraphHandle`], and inspect a flowgraph again after it has stopped.
//!
//! For custom blocks and runtime extensions, see
//! [`dev`].
use futuresdr_types::PmtConversionError;
use thiserror::Error;

use crate::runtime::channel::mpsc;
use crate::runtime::channel::oneshot;

mod add_to_flowgraph;
mod block;
mod block_inbox;
mod block_meta;
pub mod buffer;
/// Async channels used by runtime and block implementation APIs.
pub mod channel;
pub mod config;
/// Developer-facing APIs for implementing custom blocks and runtime extensions.
pub mod dev;

#[cfg(all(not(target_arch = "wasm32"), feature = "ctrl_port"))]
mod ctrl_port;
#[cfg(all(not(target_arch = "wasm32"), feature = "ctrl_port"))]
use crate::runtime::ctrl_port::ControlPort;

mod logging;

mod flowgraph;
mod flowgraph_handle;
mod flowgraph_task;
mod kernel;
mod kernel_interface;
#[cfg(not(target_arch = "wasm32"))]
mod local_domain;
#[cfg(target_arch = "wasm32")]
#[path = "local_domain_wasm.rs"]
mod local_domain;
mod local_domain_common;
mod message_output;
#[cfg(all(not(target_arch = "wasm32"), feature = "mocker"))]
/// Mocker for unit testing and benchmarking
pub mod mocker;
mod running_flowgraph;
#[allow(clippy::module_inception)]
mod runtime;
/// Advanced scheduler APIs for implementing custom executors.
pub mod scheduler;
mod tag;
mod timer;
mod work_io;
mod wrapped_kernel;

/// Macros for building flowgraphs and implementing blocks.
pub mod macros {
    pub use futuresdr_macros::Block;
    pub use futuresdr_macros::connect;
    pub use futuresdr_macros::connect_async;
}

pub use flowgraph::BlockRef;
pub use flowgraph::Flowgraph;
pub use flowgraph::LocalDomain;
pub use flowgraph::LocalDomainContext;
pub use flowgraph::TerminatedFlowgraph;
pub use flowgraph_handle::FlowgraphBlockHandle;
pub use flowgraph_handle::FlowgraphHandle;
pub use flowgraph_task::FlowgraphTask;
pub use running_flowgraph::RunningFlowgraph;
pub use runtime::DefaultScheduler;
pub use runtime::Runtime;
pub use runtime::RuntimeHandle;
pub use timer::Timer;

pub use futuresdr_types::BlockDescription;
pub use futuresdr_types::BlockId;
pub use futuresdr_types::BlockStatus;
pub use futuresdr_types::Edge;
pub use futuresdr_types::FlowgraphDescription;
pub use futuresdr_types::FlowgraphId;
pub use futuresdr_types::Pmt;
pub use futuresdr_types::PmtKind;
pub use futuresdr_types::PortId;
pub use futuresdr_types::PortIndex;
pub use futuresdr_types::PortName;

pub(crate) fn port_id_matches(port_id: &PortId, index: usize, name: &str) -> bool {
    match port_id {
        PortId::Index(port_index) => port_index.index() == index,
        PortId::Name(port_name) => port_name.as_str() == name,
    }
}

pub(crate) fn resolve_port_index<N: AsRef<str>>(
    port_id: &PortId,
    names: &[N],
) -> Option<PortIndex> {
    match port_id {
        PortId::Index(index) => (index.index() < names.len()).then_some(*index),
        PortId::Name(name) => names
            .iter()
            .position(|candidate| candidate.as_ref() == name.as_str())
            .map(PortIndex::new),
    }
}

pub(crate) fn resolve_port_name<N: AsRef<str>>(port_id: &PortId, names: &[N]) -> Option<PortId> {
    resolve_port_index(port_id, names).map(|index| PortId::new(names[index.index()].as_ref()))
}

/// Block the current thread until a future completes.
///
/// This is a small convenience wrapper around the native async executor and is
/// useful when synchronous application code needs to call async runtime APIs.
#[cfg(not(target_arch = "wasm32"))]
pub fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    async_io::block_on(future)
}

/// Proc-macro and runtime plumbing that is public only so downstream macro
/// expansions can reference generated implementation details.
#[doc(hidden)]
pub mod __private {
    pub use super::add_to_flowgraph::AddToFlowgraph;
    pub use super::kernel_interface::KernelInterface;
}

/// Generic result type used by runtime APIs and custom block kernels.
///
/// FutureSDR intentionally uses [`anyhow::Result`] for most kernel-level and
/// application-level fallible operations, since user block implementations often
/// need to return errors from arbitrary libraries.
pub type Result<T, E = anyhow::Error> = anyhow::Result<T, E>;

#[cfg(target_arch = "wasm32")]
pub(crate) fn yield_now() -> impl std::future::Future<Output = ()> + Unpin {
    WasmYieldNow(false)
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn wasm_event_loop_yield() -> impl std::future::Future<Output = ()> + Send + Unpin {
    WasmEventLoopYield(false)
}

#[cfg(target_arch = "wasm32")]
struct WasmEventLoopYield(bool);

#[cfg(target_arch = "wasm32")]
impl std::future::Future for WasmEventLoopYield {
    type Output = ();

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        if !self.0 {
            self.0 = true;
            let waker = cx.waker().clone();
            let callback = wasm_bindgen::closure::Closure::once_into_js(move || waker.wake());
            let function = wasm_bindgen::JsCast::unchecked_ref::<js_sys::Function>(&callback);
            if let Some(window) = web_sys::window() {
                let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(function, 1);
            } else if let Ok(scope) =
                wasm_bindgen::JsCast::dyn_into::<web_sys::WorkerGlobalScope>(js_sys::global())
            {
                let _ = scope.set_timeout_with_callback_and_timeout_and_arguments_0(function, 1);
            } else {
                cx.waker().wake_by_ref();
            }
            std::task::Poll::Pending
        } else {
            std::task::Poll::Ready(())
        }
    }
}

#[cfg(target_arch = "wasm32")]
struct WasmYieldNow(bool);

#[cfg(target_arch = "wasm32")]
impl std::future::Future for WasmYieldNow {
    type Output = ();

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        if !self.0 {
            self.0 = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        } else {
            std::task::Poll::Ready(())
        }
    }
}

/// Initialize global runtime services.
///
/// Applications usually do not need to call this function. Constructing a
/// [`Runtime`] calls it automatically.
///
/// Currently this initializes FutureSDR's default logging subscriber if no
/// tracing subscriber is already installed. Calling it manually is useful when
/// an application wants to use FutureSDR's re-exported `tracing` macros before
/// constructing a runtime. Repeated calls are harmless.
pub fn init() {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    logging::init();
}

/// Internal flowgraph inbox message type.
#[derive(Debug)]
pub(crate) enum FlowgraphMessage {
    /// Terminate
    Terminate,
    /// Initialize
    Initialized,
    /// Block is Done
    BlockDone {
        /// The Block that is done.
        block_id: BlockId,
    },
    /// Block Error
    BlockError {
        /// The Block that ran into an error.
        block_id: BlockId,
        /// The error returned by the block.
        error: Error,
    },
}

/// Block inbox message type
#[doc(hidden)]
#[derive(Debug)]
pub(crate) enum BlockMessage {
    /// Initialize
    Initialize,
    /// Start work after all blocks initialized.
    Start,
    /// Terminate
    Terminate,
    /// Stream input port is done
    StreamInputDone {
        /// Stream input index
        input_id: PortIndex,
    },
    /// Stream output port is done
    StreamOutputDone {
        /// Stream output index
        output_id: PortIndex,
    },
    /// Post to a message handler without waiting for handler completion.
    Post {
        /// Message handler Id
        port_id: PortIndex,
        /// [`Pmt`] input data
        data: Pmt,
    },
    /// Call a message handler and wait for its return value.
    Call {
        /// Message handler Id
        port_id: PortIndex,
        /// [`Pmt`] input data
        data: Pmt,
        /// Back channel for handler result
        tx: oneshot::Sender<Result<Pmt, Error>>,
    },
}

/// Errors returned by runtime control, connection, and validation APIs.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// A block id does not exist in the target flowgraph.
    #[error("Block {:?} does not exist", 0)]
    InvalidBlock(BlockId),
    /// The target flowgraph has already terminated or its control inbox closed.
    #[error("Flowgraph terminated")]
    FlowgraphTerminated,
    /// A message port does not exist on the referenced block.
    #[error("Block '{0:?}' does not have message port '{1:?}'")]
    InvalidMessagePort(BlockId, PortId),
    /// A stream port does not exist on the referenced block.
    #[error("Block '{0:?}' does not have stream port '{1:?}'")]
    InvalidStreamPort(BlockId, PortId),
    /// A parameter value was rejected by a runtime API.
    #[error("Invalid Parameter")]
    InvalidParameter,
    /// A message handler returned an error.
    #[error("Error in message handler: {0}")]
    HandlerError(String),
    /// The target block has already terminated.
    #[error("Block already terminated")]
    BlockTerminated,
    /// Internal runtime failure or unexpected async-channel shutdown.
    #[error("Runtime error ({0})")]
    RuntimeError(String),
    /// Flowgraph, block, or buffer validation failed before or during startup.
    #[error("Validation error {0}")]
    ValidationError(String),
    /// Conversion to or from a [`Pmt`] failed.
    #[error("PMT conversion error")]
    PmtConversionError,
    /// Conversion between Seify arguments and PMTs failed.
    #[cfg(feature = "seify")]
    #[error("Seify Args conversion error")]
    SeifyArgsConversionError,
    /// Error returned by the Seify SDR hardware abstraction layer.
    #[cfg(feature = "seify")]
    #[error("Seify error ({0})")]
    SeifyError(String),
}

impl From<anyhow::Error> for Error {
    fn from(value: anyhow::Error) -> Self {
        match value.downcast::<Self>() {
            Ok(error) => error,
            Err(error) => Self::RuntimeError(error.to_string()),
        }
    }
}

#[cfg(feature = "seify")]
impl From<seify::Error> for Error {
    fn from(value: seify::Error) -> Self {
        Error::SeifyError(value.to_string())
    }
}

impl From<oneshot::Canceled> for Error {
    fn from(_value: oneshot::Canceled) -> Self {
        Error::RuntimeError(
            "Couldn't receive from oneshot channel, sender dropped unexpectedly".to_string(),
        )
    }
}

impl From<mpsc::SendError> for Error {
    fn from(_value: mpsc::SendError) -> Self {
        Error::RuntimeError(
            "Couldn't send to mpsc channel, receiver dropped unexpectedly".to_string(),
        )
    }
}

impl<T> From<mpsc::TrySendError<T>> for Error {
    fn from(_value: mpsc::TrySendError<T>) -> Self {
        let message = match _value {
            mpsc::TrySendError::Full(_) => "Couldn't send to mpsc channel, channel is full",
            mpsc::TrySendError::Disconnected(_) => {
                "Couldn't send to mpsc channel, receiver dropped unexpectedly"
            }
        };
        Error::RuntimeError(message.to_string())
    }
}

impl From<PmtConversionError> for Error {
    fn from(_value: PmtConversionError) -> Self {
        Error::PmtConversionError
    }
}
