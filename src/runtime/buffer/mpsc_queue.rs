//! Thread-safe, bounded queue buffer with one writer and multiple readers.
//!
//! Readers share immutable pages, including a writer-prepared history prefix.
//! A slow reader backpressures the writer. As with other fanout buffers, one
//! downstream block finishing stops upstream production for every reader.

use super::mpsc_queued;

/// Thread-safe fanout queue reader.
pub type Reader<D> = mpsc_queued::Reader<D, mpsc_queued::SendState<D>>;

/// Thread-safe fanout queue writer.
pub type Writer<D> = mpsc_queued::Writer<D, mpsc_queued::SendState<D>>;
