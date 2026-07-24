//! Queue-backed CPU buffer.

use crate::runtime::buffer::queued;

/// Slab reader.
pub type Reader<D> = queued::Reader<D, queued::SendState<D>>;

/// Slab writer.
pub type Writer<D> = queued::Writer<D, queued::SendState<D>>;
