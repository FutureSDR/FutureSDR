#![warn(missing_docs)]
//! # FutureSDR Types
//!
//! Shared serializable types used by the FutureSDR runtime, control port, and
//! remote clients.
//!
//! The most common type is [`Pmt`], the polymorphic message value passed to
//! message handlers and serialized by the REST API. The id and description
//! types are stable shapes for inspecting and controlling flowgraphs without
//! depending on concrete block Rust types.
mod description;
pub use description::BlockDescription;
pub use description::BlockStatus;
pub use description::Edge;
pub use description::FlowgraphDescription;

mod pmt;
pub use pmt::Pmt;
pub use pmt::PmtConversionError;
pub use pmt::PmtKind;

mod block_id;
pub use block_id::BlockId;
mod flowgraph_id;
pub use flowgraph_id::FlowgraphId;
mod port_id;
pub use port_id::PortId;
pub use port_id::PortIndex;
pub use port_id::PortName;
#[cfg(feature = "seify")]
mod seify;
