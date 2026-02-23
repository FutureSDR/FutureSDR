use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;

use futures::task::AtomicWaker;

#[derive(Debug)]
pub struct BlockNotifyState {
    pending: AtomicBool,
    waker: AtomicWaker,
}

impl Default for BlockNotifyState {
    fn default() -> Self {
        Self {
            pending: AtomicBool::new(false),
            waker: AtomicWaker::new(),
        }
    }
}

#[derive(Clone, Debug)]
/// Coalescing wakeup handle for a block.
pub struct BlockNotifier {
    state: Arc<BlockNotifyState>,
}

impl BlockNotifier {
    /// Create a new notifier.
    pub fn new() -> Self {
        Self {
            state: Arc::new(BlockNotifyState::default()),
        }
    }

    /// Notify the block once.
    ///
    /// Multiple notify calls before the block observes the signal are coalesced.
    pub fn notify(&self) {
        if !self.state.pending.swap(true, Ordering::AcqRel) {
            self.state.waker.wake();
        }
    }

    /// Consume a pending notification bit.
    pub fn take_pending(&self) -> bool {
        self.state.pending.swap(false, Ordering::AcqRel)
    }

    /// Future that resolves on the next pending notification.
    pub fn notified(&self) -> Notified {
        Notified {
            state: self.state.clone(),
        }
    }
}

pub struct Notified {
    state: Arc<BlockNotifyState>,
}

impl Future for Notified {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.state.pending.swap(false, Ordering::AcqRel) {
            return Poll::Ready(());
        }

        self.state.waker.register(cx.waker());

        if self.state.pending.swap(false, Ordering::AcqRel) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;

    #[test]
    fn coalesces_multiple_notifies() {
        let n = BlockNotifier::new();
        n.notify();
        n.notify();
        n.notify();

        assert!(n.take_pending());
        assert!(!n.take_pending());
    }

    #[test]
    fn notified_completes_after_notify() {
        let n = BlockNotifier::new();
        n.notify();
        block_on(n.notified());
        assert!(!n.take_pending());
    }
}
