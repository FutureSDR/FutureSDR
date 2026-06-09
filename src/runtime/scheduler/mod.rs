//! Scheduler trait and built-in scheduler implementations.
//!
//! Schedulers execute normal flowgraph block tasks and general async tasks
//! spawned through [`crate::runtime::Runtime`]. Most applications use the
//! default scheduler selected by `Runtime::new`; custom schedulers implement
//! [`crate::runtime::scheduler::Scheduler`].

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
pub use local::LocalBlockStop;
pub use local::LocalDomainControl;
pub use local::LocalDomainRunEvent;
pub use local::LocalDomainRunSpec;
pub use local::LocalScheduler;
pub use local::RunnableLocalBlock;
pub use local::StoppedLocalBlock;
#[allow(clippy::module_inception)]
mod scheduler;
pub use scheduler::DomainTopology;
pub(crate) use scheduler::LocalDomainSpec;
pub use scheduler::NormalBlock;
pub use scheduler::NormalBlocks;
pub use scheduler::NormalDomainSpec;
pub use scheduler::NormalRunningDomain;
pub(crate) use scheduler::RunningDomain;
pub use scheduler::Scheduler;
pub(crate) use scheduler::StoppedDomain;
pub(crate) use scheduler::StoppedDomainState;

#[cfg(target_arch = "wasm32")]
pub mod wasm;
#[cfg(target_arch = "wasm32")]
pub use wasm::WasmMainScheduler;
#[cfg(target_arch = "wasm32")]
pub use wasm::WasmScheduler;

#[cfg(not(target_arch = "wasm32"))]
pub use async_task::Task;
#[cfg(target_arch = "wasm32")]
pub use wasm::Task;
