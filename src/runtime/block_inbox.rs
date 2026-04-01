use futures::SinkExt;
use futures::StreamExt;
use futures::task::AtomicWaker;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;

use crate::channel::mpsc;
use crate::runtime::BlockMessage;

#[derive(Debug)]
struct BlockNotifyState {
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

/// Coalescing wakeup handle for a block.
#[derive(Clone, Debug)]
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

impl Default for BlockNotifier {
    fn default() -> Self {
        Self::new()
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

#[derive(Clone, Debug)]
/// Sender-side actor inbox for blocks.
pub struct BlockInbox {
    control: mpsc::Sender<BlockMessage>,
    notifier: BlockNotifier,
}

impl BlockInbox {
    /// Create a sender-side block inbox from an mpsc sender and notifier.
    pub fn new(control: mpsc::Sender<BlockMessage>, notifier: BlockNotifier) -> Self {
        Self { control, notifier }
    }

    /// Create an inbox that is disconnected from any reader.
    pub fn disconnected() -> Self {
        let (control, _) = mpsc::channel(0);
        Self::new(control, BlockNotifier::new())
    }

    /// Get a wake-only notifier for the destination block.
    pub fn notifier(&self) -> BlockNotifier {
        self.notifier.clone()
    }

    /// Wake the destination block without sending a message.
    pub fn notify(&self) {
        self.notifier.notify();
    }

    /// Return whether the underlying receiver has been closed.
    pub fn is_closed(&self) -> bool {
        self.control.is_closed()
    }

    /// Enqueue a block message and wake the destination block on success.
    pub async fn send(
        &mut self,
        msg: BlockMessage,
    ) -> Result<(), futures::channel::mpsc::SendError> {
        self.control.send(msg).await?;
        self.notifier.notify();
        Ok(())
    }
}

impl Default for BlockInbox {
    fn default() -> Self {
        Self::disconnected()
    }
}

#[derive(Debug)]
/// Receiver-side actor inbox for blocks.
pub struct BlockInboxReader {
    control: mpsc::Receiver<BlockMessage>,
    notifier: BlockNotifier,
}

impl BlockInboxReader {
    /// Create a receiver-side block inbox from an mpsc receiver and notifier.
    pub fn new(control: mpsc::Receiver<BlockMessage>, notifier: BlockNotifier) -> Self {
        Self { control, notifier }
    }

    /// Try to receive a queued block message without blocking.
    pub fn try_recv(&mut self) -> Option<BlockMessage> {
        self.control.try_recv().ok()
    }

    /// Wait for the next queued block message.
    pub async fn recv(&mut self) -> Option<BlockMessage> {
        self.control.next().await
    }

    /// Consume a pending wakeup notification bit.
    pub fn take_pending(&self) -> bool {
        self.notifier.take_pending()
    }

    /// Future that resolves when the block is woken.
    pub fn notified(&self) -> Notified {
        self.notifier.notified()
    }
}

/// Create a paired sender/reader block inbox with a coalescing notifier.
pub fn channel(size: usize) -> (BlockInbox, BlockInboxReader) {
    let (control, receiver) = mpsc::channel(size);
    let notifier = BlockNotifier::new();
    (
        BlockInbox::new(control, notifier.clone()),
        BlockInboxReader::new(receiver, notifier),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::BlockMessage;
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

    #[test]
    fn send_enqueues_and_wakes_reader() {
        let (mut tx, mut rx) = channel(1);

        block_on(tx.send(BlockMessage::Initialize)).unwrap();

        assert!(rx.take_pending());
        assert!(matches!(rx.try_recv(), Some(BlockMessage::Initialize)));
    }

    #[test]
    fn recv_waits_for_message() {
        let (mut tx, mut rx) = channel(1);

        block_on(tx.send(BlockMessage::Initialize)).unwrap();

        assert!(matches!(
            block_on(rx.recv()),
            Some(BlockMessage::Initialize)
        ));
    }

    #[test]
    fn notify_wakes_without_message() {
        let (tx, mut rx) = channel(1);

        tx.notify();

        assert!(rx.take_pending());
        assert!(rx.try_recv().is_none());
    }

    #[test]
    fn multiple_sends_coalesce_but_keep_messages() {
        let (mut tx, mut rx) = channel(4);

        block_on(tx.send(BlockMessage::Initialize)).unwrap();
        block_on(tx.send(BlockMessage::Terminate)).unwrap();

        assert!(rx.take_pending());
        assert!(!rx.take_pending());
        assert!(matches!(rx.try_recv(), Some(BlockMessage::Initialize)));
        assert!(matches!(rx.try_recv(), Some(BlockMessage::Terminate)));
        assert!(rx.try_recv().is_none());
    }
}
