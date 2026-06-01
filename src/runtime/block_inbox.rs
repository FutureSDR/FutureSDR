use futures::task::AtomicWaker;
use std::cell::Cell;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use crate::runtime::BlockMessage;
use crate::runtime::PortId;
use crate::runtime::channel::mpsc;
use crate::runtime::local_domain::LocalDomainInbox;

#[derive(Debug)]
struct ThreadSafeNotifyState {
    pending: AtomicBool,
    message_pending: AtomicBool,
    waker: AtomicWaker,
}

impl Default for ThreadSafeNotifyState {
    fn default() -> Self {
        Self {
            pending: AtomicBool::new(false),
            message_pending: AtomicBool::new(false),
            waker: AtomicWaker::new(),
        }
    }
}

/// Coalescing wakeup handle for a normal thread-safe block.
///
/// A notifier wakes the block without carrying a message payload. Repeated
/// notifications are coalesced until the block observes the pending wakeup.
#[derive(Clone)]
pub struct BlockNotifier {
    state: Arc<ThreadSafeNotifyState>,
}

impl fmt::Debug for BlockNotifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlockNotifier").finish_non_exhaustive()
    }
}

impl BlockNotifier {
    /// Create a new thread-safe notifier.
    pub fn new() -> Self {
        Self {
            state: Arc::new(ThreadSafeNotifyState::default()),
        }
    }

    /// Notify the block once.
    ///
    /// Multiple notify calls before the block observes the signal are coalesced.
    #[inline(always)]
    pub fn notify(&self) {
        if !self.state.pending.swap(true, Ordering::AcqRel) {
            self.state.waker.wake();
        }
    }

    /// Consume a pending notification bit.
    #[inline(always)]
    pub fn take_pending(&self) -> bool {
        self.state.pending.swap(false, Ordering::AcqRel)
    }

    /// Return a future that resolves on the next pending notification.
    pub fn notified(&self) -> Notified {
        Notified {
            state: self.state.clone(),
        }
    }

    fn set_message_pending(&self) {
        self.state.message_pending.store(true, Ordering::Release);
    }

    fn take_message_pending(&self) -> bool {
        self.state.message_pending.swap(false, Ordering::AcqRel)
    }
}

impl Default for BlockNotifier {
    fn default() -> Self {
        Self::new()
    }
}

/// Future returned by [`BlockNotifier::notified`].
///
/// Polling this future consumes one pending notification bit.
pub struct Notified {
    state: Arc<ThreadSafeNotifyState>,
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

/// Sender-side actor inbox for a block.
///
/// Normal-domain blocks expose a direct thread-safe inbox. Local-domain blocks
/// expose a domain proxy for runtime/control/message ingress; the domain owns
/// and forwards into the private local inbox used by the block task.
#[doc(hidden)]
#[derive(Clone)]
pub struct ThreadSafeBlockInbox {
    tx: mpsc::Sender<BlockMessage>,
    notifier: BlockNotifier,
}

/// Sender-side actor inbox variants.
#[derive(Clone)]
pub enum BlockInbox {
    /// Direct thread-safe inbox for a normal-domain block.
    #[doc(hidden)]
    ThreadSafe(ThreadSafeBlockInbox),
    /// Send-safe proxy to a local-domain block.
    #[doc(hidden)]
    DomainProxy {
        domain: LocalDomainInbox,
        block_id: crate::runtime::BlockId,
    },
}

impl fmt::Debug for BlockInbox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ThreadSafe(_) => f
                .debug_struct("BlockInbox::ThreadSafe")
                .finish_non_exhaustive(),
            Self::DomainProxy { block_id, .. } => f
                .debug_struct("BlockInbox::DomainProxy")
                .field("block_id", block_id)
                .finish_non_exhaustive(),
        }
    }
}

impl BlockInbox {
    /// Create a sender-side thread-safe block inbox from an mpsc sender and notifier.
    pub(crate) fn new(control: mpsc::Sender<BlockMessage>, notifier: BlockNotifier) -> Self {
        Self::ThreadSafe(ThreadSafeBlockInbox {
            tx: control,
            notifier,
        })
    }

    /// Create a sender-side domain proxy for a local-domain block.
    pub(crate) fn domain_proxy(
        domain: LocalDomainInbox,
        block_id: crate::runtime::BlockId,
    ) -> Self {
        Self::DomainProxy { domain, block_id }
    }

    /// Get a wake-only notifier for the destination block.
    #[inline(always)]
    pub fn notifier(&self) -> BlockNotifier {
        match self {
            Self::ThreadSafe(thread_safe) => thread_safe.notifier.clone(),
            Self::DomainProxy { .. } => BlockNotifier::new(),
        }
    }

    /// Wake the destination block without sending a message.
    #[inline(always)]
    pub fn notify(&self) {
        match self {
            Self::ThreadSafe(thread_safe) => thread_safe.notifier.notify(),
            Self::DomainProxy { domain, block_id } => {
                let _ = domain.notify_block(*block_id);
            }
        }
    }

    /// Return whether the underlying receiver has been closed.
    pub fn is_closed(&self) -> bool {
        match self {
            Self::ThreadSafe(thread_safe) => thread_safe.tx.is_closed(),
            Self::DomainProxy { domain, .. } => domain.is_closed(),
        }
    }

    /// Notify the destination block that one stream input port is done.
    pub async fn stream_input_done(&self, input_id: PortId) -> Result<(), crate::runtime::Error> {
        self.send(BlockMessage::StreamInputDone { input_id }).await
    }

    /// Notify the destination block that one stream output port is done.
    pub async fn stream_output_done(&self, output_id: PortId) -> Result<(), crate::runtime::Error> {
        self.send(BlockMessage::StreamOutputDone { output_id })
            .await
    }

    /// Enqueue a block message and wake the destination block on success.
    pub(crate) async fn send(&self, msg: BlockMessage) -> Result<(), crate::runtime::Error> {
        match self {
            Self::ThreadSafe(thread_safe) => {
                thread_safe.tx.send(msg).await?;
                thread_safe.notifier.set_message_pending();
                thread_safe.notifier.notify();
                Ok(())
            }
            Self::DomainProxy { domain, block_id } => match msg {
                BlockMessage::Call { port_id, data, tx } => {
                    domain.call(*block_id, port_id, data, tx).await
                }
                msg => domain.post(*block_id, msg).await,
            },
        }
    }
}

/// Receiver-side actor inbox for normal thread-safe blocks.
#[derive(Debug)]
pub struct BlockInboxReader {
    control: mpsc::Receiver<BlockMessage>,
    notifier: BlockNotifier,
}

impl BlockInboxReader {
    /// Create a receiver-side block inbox from an mpsc receiver and notifier.
    pub(crate) fn new(control: mpsc::Receiver<BlockMessage>, notifier: BlockNotifier) -> Self {
        Self { control, notifier }
    }

    /// Try to receive a queued block message without blocking.
    pub(crate) fn try_recv(&mut self) -> Option<BlockMessage> {
        self.control.try_recv().ok()
    }

    /// Wait for the next queued block message.
    pub(crate) async fn recv(&mut self) -> Option<BlockMessage> {
        self.control.recv().await
    }

    /// Consume a pending message bit.
    pub fn take_message_pending(&self) -> bool {
        self.notifier.take_message_pending()
    }

    /// Consume a pending wakeup notification bit.
    pub fn take_pending(&self) -> bool {
        self.notifier.take_pending()
    }

    /// Future that resolves when the block is woken.
    #[allow(dead_code)]
    pub fn notified(&self) -> Notified {
        self.notifier.notified()
    }
}

/// Create a paired sender/reader block inbox with a coalescing notifier.
pub(crate) fn channel(size: usize) -> (BlockInbox, BlockInboxReader) {
    let (control, receiver) = mpsc::channel::<BlockMessage>(size);
    let notifier = BlockNotifier::new();
    (
        BlockInbox::new(control, notifier.clone()),
        BlockInboxReader::new(receiver, notifier),
    )
}

#[derive(Debug, Default)]
struct LocalNotifyState {
    pending: Cell<bool>,
    has_waker: Cell<bool>,
    waker: RefCell<Option<Waker>>,
}

/// Coalescing wakeup handle for local-domain blocks.
#[derive(Clone, Debug)]
pub struct LocalBlockNotifier(Rc<LocalNotifyState>);

impl LocalBlockNotifier {
    fn new() -> Self {
        Self(Rc::new(LocalNotifyState::default()))
    }

    /// Notify the local block once.
    #[inline(always)]
    pub fn notify(&self) {
        self.0.notify();
    }

    #[inline(always)]
    fn take_pending(&self) -> bool {
        self.0.take_pending()
    }

    #[inline(always)]
    fn set_waker(&self, waker: Waker) {
        self.0.set_waker(waker);
    }
}

impl Default for LocalBlockNotifier {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalNotifyState {
    #[inline(always)]
    fn notify(&self) {
        if !self.pending.replace(true)
            && self.has_waker.replace(false)
            && let Some(waker) = self.waker.borrow_mut().take()
        {
            waker.wake();
        }
    }

    #[inline(always)]
    fn set_waker(&self, waker: Waker) {
        *self.waker.borrow_mut() = Some(waker);
        self.has_waker.set(true);
    }

    #[inline(always)]
    fn take_pending(&self) -> bool {
        self.pending.replace(false)
    }
}

#[derive(Debug)]
struct LocalInboxState {
    queue: RefCell<VecDeque<BlockMessage>>,
    message_pending: Cell<bool>,
    notifier: LocalBlockNotifier,
}

/// Sender-side actor inbox for blocks running inside one local domain.
#[derive(Clone, Debug)]
pub struct LocalBlockInbox(Rc<LocalInboxState>);

/// Handle used by the local-domain dispatcher for direct delivery.
pub type LocalInboxHandle = LocalBlockInbox;

thread_local! {
    static LOCAL_DISPATCH_CONTEXT: RefCell<Option<Vec<Option<LocalInboxHandle>>>> = const { RefCell::new(None) };
}

/// Guard returned while a local domain dispatch context is installed.
pub(crate) struct LocalDispatchContextGuard {
    previous: Option<Vec<Option<LocalInboxHandle>>>,
}

impl Drop for LocalDispatchContextGuard {
    fn drop(&mut self) {
        LOCAL_DISPATCH_CONTEXT.with(|context| {
            context.replace(self.previous.take());
        });
    }
}

/// Install a domain-local dispatch context for direct local message delivery.
pub(crate) fn enter_local_dispatch_context(
    inboxes: Vec<Option<LocalInboxHandle>>,
) -> LocalDispatchContextGuard {
    let previous = LOCAL_DISPATCH_CONTEXT.with(|context| context.replace(Some(inboxes)));
    LocalDispatchContextGuard { previous }
}

/// Push a message to a local-domain inbox through the current dispatch context.
pub(crate) fn push_current_local_message(
    local_id: usize,
    message: BlockMessage,
) -> Result<(), crate::runtime::Error> {
    let inbox = LOCAL_DISPATCH_CONTEXT.with(|context| {
        context
            .borrow()
            .as_ref()
            .and_then(|inboxes| inboxes.get(local_id).cloned().flatten())
    });

    let inbox = inbox.ok_or_else(|| {
        crate::runtime::Error::RuntimeError(format!(
            "local message handler has no dispatch inbox for local block slot {local_id}"
        ))
    })?;
    inbox.push(message);
    Ok(())
}

impl LocalInboxState {
    fn new(notifier: LocalBlockNotifier) -> Self {
        Self {
            queue: RefCell::new(VecDeque::new()),
            message_pending: Cell::new(false),
            notifier,
        }
    }
}

impl LocalBlockInbox {
    pub(crate) fn notify(&self) {
        self.0.notifier.notify();
    }

    pub(crate) fn push(&self, msg: BlockMessage) {
        self.0.queue.borrow_mut().push_back(msg);
        self.0.message_pending.set(true);
        self.notify();
    }

    /// Notify the destination local block that one stream input port is done.
    pub async fn stream_input_done(&self, input_id: PortId) -> Result<(), crate::runtime::Error> {
        self.push(BlockMessage::StreamInputDone { input_id });
        Ok(())
    }

    /// Notify the destination local block that one stream output port is done.
    pub async fn stream_output_done(&self, output_id: PortId) -> Result<(), crate::runtime::Error> {
        self.push(BlockMessage::StreamOutputDone { output_id });
        Ok(())
    }

    /// Get a wake-only notifier for this local block.
    pub fn notifier(&self) -> LocalBlockNotifier {
        self.0.notifier.clone()
    }

    fn try_recv(&self) -> Option<BlockMessage> {
        self.0.queue.borrow_mut().pop_front()
    }

    fn take_message_pending(&self) -> bool {
        self.0.message_pending.replace(false)
    }
}

/// Receiver-side actor inbox for local-domain blocks.
#[derive(Debug)]
pub(crate) struct LocalBlockInboxReader {
    state: LocalBlockInbox,
}

impl LocalBlockInboxReader {
    pub(crate) fn pair() -> (LocalBlockInbox, LocalBlockInboxReader, LocalInboxHandle) {
        let notifier = LocalBlockNotifier::new();
        let state = LocalBlockInbox(Rc::new(LocalInboxState::new(notifier)));
        (
            state.clone(),
            LocalBlockInboxReader {
                state: state.clone(),
            },
            state,
        )
    }

    /// Try to receive a queued block message without blocking.
    pub fn try_recv(&mut self) -> Option<BlockMessage> {
        self.state.try_recv()
    }

    /// Wait for the next queued block message.
    pub async fn recv(&mut self) -> Option<BlockMessage> {
        loop {
            if let Some(msg) = self.state.try_recv() {
                return Some(msg);
            }
            self.notified().await;
        }
    }

    /// Consume a pending message bit.
    pub fn take_message_pending(&self) -> bool {
        self.state.take_message_pending()
    }

    /// Consume a pending wakeup notification bit.
    pub fn take_pending(&self) -> bool {
        self.state.0.notifier.take_pending()
    }

    /// Future that resolves when the block is woken.
    pub fn notified(&self) -> LocalNotified {
        LocalNotified {
            state: self.state.0.notifier.clone(),
        }
    }
}

/// Future returned by [`LocalBlockInboxReader::notified`].
pub struct LocalNotified {
    state: LocalBlockNotifier,
}

impl Future for LocalNotified {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.state.take_pending() {
            return Poll::Ready(());
        }

        self.state.set_waker(cx.waker().clone());

        if self.state.take_pending() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
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
    fn local_coalesces_multiple_notifies() {
        let (_, rx, _) = LocalBlockInboxReader::pair();
        let n = rx.state.notifier();
        n.notify();
        n.notify();
        n.notify();

        assert!(rx.take_pending());
        assert!(!rx.take_pending());
    }

    #[test]
    fn notified_completes_after_notify() {
        let n = BlockNotifier::new();
        n.notify();
        block_on(n.notified());
        assert!(!n.take_pending());
    }

    #[test]
    fn local_notified_completes_after_notify() {
        let (_, rx, _) = LocalBlockInboxReader::pair();
        let n = rx.state.notifier();
        n.notify();
        block_on(rx.notified());
        assert!(!rx.take_pending());
    }

    #[test]
    fn send_enqueues_and_wakes_reader() {
        let (tx, mut rx) = channel(1);

        block_on(tx.send(BlockMessage::Initialize)).unwrap();

        assert!(rx.take_pending());
        assert!(rx.take_message_pending());
        assert!(!rx.take_message_pending());
        assert!(matches!(rx.try_recv(), Some(BlockMessage::Initialize)));
    }

    #[test]
    fn local_send_enqueues_and_wakes_reader() {
        let (tx, mut rx, _) = LocalBlockInboxReader::pair();

        tx.push(BlockMessage::Initialize);

        assert!(rx.take_pending());
        assert!(rx.take_message_pending());
        assert!(!rx.take_message_pending());
        assert!(matches!(rx.try_recv(), Some(BlockMessage::Initialize)));
    }

    #[test]
    fn recv_waits_for_message() {
        let (tx, mut rx) = channel(1);

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
        assert!(!rx.take_message_pending());
        assert!(rx.try_recv().is_none());
    }

    #[test]
    fn local_notify_wakes_without_message() {
        let (tx, mut rx, _) = LocalBlockInboxReader::pair();

        tx.notify();

        assert!(rx.take_pending());
        assert!(!rx.take_message_pending());
        assert!(rx.try_recv().is_none());
    }

    #[test]
    fn multiple_sends_coalesce_but_keep_messages() {
        let (tx, mut rx) = channel(4);

        block_on(tx.send(BlockMessage::Initialize)).unwrap();
        block_on(tx.send(BlockMessage::Terminate)).unwrap();

        assert!(rx.take_pending());
        assert!(rx.take_message_pending());
        assert!(!rx.take_pending());
        assert!(!rx.take_message_pending());
        assert!(matches!(rx.try_recv(), Some(BlockMessage::Initialize)));
        assert!(matches!(rx.try_recv(), Some(BlockMessage::Terminate)));
        assert!(rx.try_recv().is_none());
    }
}
