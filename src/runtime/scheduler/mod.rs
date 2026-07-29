//! Scheduler trait and built-in scheduler implementations.
//!
//! Schedulers execute normal flowgraph block tasks and general async tasks
//! spawned through [`crate::runtime::Runtime`]. Most applications use the
//! default scheduler selected by `Runtime::new`; custom schedulers implement
//! [`Scheduler`](crate::runtime::scheduler::Scheduler) or
//! [`LocalScheduler`](crate::runtime::scheduler::LocalScheduler). The support
//! types required by custom implementations are grouped under
//! [`dev`](crate::runtime::scheduler::dev).

#[cfg(feature = "flow_scheduler")]
mod flow;
#[cfg(feature = "flow_scheduler")]
pub use crate::runtime::scheduler::flow::FlowScheduler;

#[cfg(not(target_arch = "wasm32"))]
mod smol;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::runtime::scheduler::smol::SmolScheduler;
mod local;
pub use local::BasicLocalScheduler;
pub use local::LocalScheduler;
#[allow(clippy::module_inception)]
mod scheduler;
pub(crate) use scheduler::LocalDomainSpec;
pub(crate) use scheduler::LocalRunningDomain;
pub(crate) use scheduler::NormalBlocks;
pub use scheduler::Scheduler;

/// Support types for implementing custom schedulers.
pub mod dev {
    pub use super::local::LocalBlockStop;
    pub use super::local::LocalDomainRunSpec;
    pub use super::local::RunnableLocalBlock;
    pub use super::local::StoppedLocalBlock;
    pub use super::scheduler::BlockStop;
    pub use super::scheduler::DomainTopology;
    pub use super::scheduler::NormalDomainSpec;
    pub use super::scheduler::NormalRunningDomain;
    pub use super::scheduler::RunnableBlock;
    pub use super::scheduler::StoppedBlock;
}

#[cfg(target_arch = "wasm32")]
pub mod wasm;
#[cfg(target_arch = "wasm32")]
pub use wasm::WasmMainScheduler;
#[cfg(target_arch = "wasm32")]
pub use wasm::WasmScheduler;

#[doc(no_inline)]
pub use async_task::Task;
