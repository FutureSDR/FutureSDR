//! Buffer implementation for Burn tensors.
//!
//! Burn tensors may take ownership of a buffer's backing data. The in-place
//! implementation therefore recycles capacity permits on drop and allocates a
//! fresh tensor/data buffer when a returned permit is used again.
#[allow(clippy::module_inception)]
mod burn;
pub use burn::Buffer;
pub use burn::Reader;
pub use burn::Writer;
