//! Same-domain queue buffer with one writer and multiple readers.
//!
//! Page ownership and queue state use non-atomic reference counting. All
//! endpoints must stay in the same local domain.

use super::LocalBlockInbox;
use super::mpsc_queued;

/// Same-domain fanout queue reader.
pub type Reader<D> = mpsc_queued::Reader<D, mpsc_queued::LocalState<D>, LocalBlockInbox>;

/// Same-domain fanout queue writer.
pub type Writer<D> = mpsc_queued::Writer<D, mpsc_queued::LocalState<D>, LocalBlockInbox>;
