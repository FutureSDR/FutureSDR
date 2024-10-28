//! Flowgraph Scheduler Trait and Implementations
#[cfg(feature = "flow_scheduler")]
mod cpu_pin;
#[cfg(feature = "flow_scheduler")]
pub use crate::runtime::scheduler::cpu_pin::CpuPinScheduler;

#[cfg(feature = "flow_scheduler")]
mod flow;
#[cfg(feature = "flow_scheduler")]
pub use crate::runtime::scheduler::flow::FlowScheduler;

#[cfg(not(target_arch = "wasm32"))]
mod smol;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::runtime::scheduler::smol::SmolScheduler;

#[cfg(feature = "tpb_scheduler")]
mod tpb;
#[cfg(feature = "tpb_scheduler")]
pub use crate::runtime::scheduler::tpb::TpbScheduler;

#[allow(clippy::module_inception)]
mod scheduler;
pub use scheduler::Scheduler;

#[cfg(target_arch = "wasm32")]
pub mod wasm;
#[cfg(target_arch = "wasm32")]
pub use wasm::WasmScheduler;

#[cfg(not(target_arch = "wasm32"))]
pub use async_task::Task;
#[cfg(target_arch = "wasm32")]
pub use wasm::Task;
