#[cfg(not(target_arch = "wasm32"))]
mod flow;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::runtime::scheduler::flow::FlowScheduler;

#[cfg(not(target_arch = "wasm32"))]
mod smol;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::runtime::scheduler::smol::SmolScheduler;

#[cfg(not(target_arch = "wasm32"))]
mod tpb;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::runtime::scheduler::tpb::TpbScheduler;

#[allow(clippy::module_inception)]
mod scheduler;
pub use scheduler::Scheduler;

#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(target_arch = "wasm32")]
pub use wasm::WasmScheduler;
