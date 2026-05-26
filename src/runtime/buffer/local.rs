//! Same-thread CPU buffer for local domains.

use crate::runtime::buffer::LocalMode;
use crate::runtime::buffer::queued;

/// Same-thread CPU reader.
pub type Reader<D> = queued::Reader<D, queued::LocalState<D>, LocalMode>;

/// Same-thread CPU writer.
pub type Writer<D> = queued::Writer<D, queued::LocalState<D>, LocalMode>;
