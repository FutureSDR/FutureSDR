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

use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
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

/// Concrete send-capable actor inbox for a normal-domain block.
///
/// Local-domain blocks keep one of these for cross-domain buffer finish
/// notifications, while runtime/control/message ingress is routed through a
/// [`BlockEndpoint::DomainProxy`].
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct BlockInbox {
    tx: mpsc::Sender<BlockMessage>,
    notifier: BlockNotifier,
}

/// Send-safe endpoint for routing messages to a block.
///
/// Normal-domain blocks use a direct concrete inbox. Local-domain blocks expose
/// a domain proxy; the domain owns and forwards into the private local inbox
/// used by the block task.
#[derive(Clone)]
pub enum BlockEndpoint {
    /// Direct concrete inbox for a normal-domain block.
    #[doc(hidden)]
    Direct(BlockInbox),
    /// Send-safe proxy to a local-domain block.
    #[doc(hidden)]
    DomainProxy {
        domain: LocalDomainInbox,
        block_id: BlockId,
    },
}

impl fmt::Debug for BlockEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Direct(_) => f
                .debug_struct("BlockEndpoint::Direct")
                .finish_non_exhaustive(),
            Self::DomainProxy { block_id, .. } => f
                .debug_struct("BlockEndpoint::DomainProxy")
                .field("block_id", block_id)
                .finish_non_exhaustive(),
        }
    }
}

impl BlockInbox {
    /// Create a sender-side thread-safe block inbox from an mpsc sender and notifier.
    pub(crate) fn new(control: mpsc::Sender<BlockMessage>, notifier: BlockNotifier) -> Self {
        Self {
            tx: control,
            notifier,
        }
    }

    /// Get a wake-only notifier for the destination block.
    #[inline(always)]
    pub fn notifier(&self) -> BlockNotifier {
        self.notifier.clone()
    }

    /// Wake the destination block without sending a message.
    #[inline(always)]
    pub fn notify(&self) {
        self.notifier.notify();
    }

    /// Notify the destination block that one stream input port is done.
    pub async fn stream_input_done(&self, input_id: PortId) -> Result<(), Error> {
        self.send(BlockMessage::StreamInputDone { input_id }).await
    }

    /// Notify the destination block that one stream output port is done.
    pub async fn stream_output_done(&self, output_id: PortId) -> Result<(), Error> {
        self.send(BlockMessage::StreamOutputDone { output_id })
            .await
    }

    /// Enqueue a block message and wake the destination block on success.
    pub(crate) async fn send(&self, msg: BlockMessage) -> Result<(), Error> {
        self.tx.send(msg).await?;
        self.notifier.set_message_pending();
        self.notifier.notify();
        Ok(())
    }
}

impl From<BlockInbox> for BlockEndpoint {
    fn from(inbox: BlockInbox) -> Self {
        Self::Direct(inbox)
    }
}

impl BlockEndpoint {
    /// Create a sender-side domain proxy for a local-domain block.
    pub(crate) fn domain_proxy(domain: LocalDomainInbox, block_id: BlockId) -> Self {
        Self::DomainProxy { domain, block_id }
    }

    /// Wake the destination block without sending a message.
    #[inline(always)]
    pub fn notify(&self) {
        match self {
            Self::Direct(inbox) => inbox.notify(),
            Self::DomainProxy { domain, block_id } => {
                let _ = domain.notify_block(*block_id);
            }
        }
    }

    /// Return whether the underlying receiver has been closed.
    pub fn is_closed(&self) -> bool {
        match self {
            Self::Direct(inbox) => inbox.tx.is_closed(),
            Self::DomainProxy { domain, .. } => domain.is_closed(),
        }
    }

    /// Enqueue a block message and wake the destination block on success.
    pub(crate) async fn send(&self, msg: BlockMessage) -> Result<(), Error> {
        match self {
            Self::Direct(inbox) => inbox.send(msg).await,
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
    pub fn notified(&self) -> Notified {
        self.notifier.notified()
    }
}

/// Create a paired concrete thread-safe sender/reader block inbox with a coalescing notifier.
pub(crate) fn thread_safe_channel(size: usize) -> (BlockInbox, BlockInboxReader) {
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

thread_local! {
    static LOCAL_DISPATCH_CONTEXT: RefCell<Option<Vec<Option<LocalBlockInbox>>>> = const { RefCell::new(None) };
}

/// Guard returned while a local domain dispatch context is installed.
pub(crate) struct LocalDispatchContextGuard {
    previous: Option<Vec<Option<LocalBlockInbox>>>,
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
    inboxes: Vec<Option<LocalBlockInbox>>,
) -> LocalDispatchContextGuard {
    let previous = LOCAL_DISPATCH_CONTEXT.with(|context| context.replace(Some(inboxes)));
    LocalDispatchContextGuard { previous }
}

/// Deliver a message to a local-domain inbox through the current dispatch context.
pub(crate) fn deliver_local_message(local_id: usize, message: BlockMessage) -> Result<(), Error> {
    LOCAL_DISPATCH_CONTEXT.with(|context| {
        let context = context.borrow();
        let inbox = context
            .as_ref()
            .and_then(|inboxes| inboxes.get(local_id))
            .and_then(Option::as_ref)
            .ok_or_else(|| {
                Error::RuntimeError(format!(
                    "local message handler has no dispatch inbox for local block slot {local_id}"
                ))
            })?;
        inbox.push(message);
        Ok(())
    })
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
    inbox: LocalBlockInbox,
}

impl LocalBlockInboxReader {
    /// Create a local sender/reader pair sharing one inbox queue and notifier.
    pub(crate) fn pair() -> (LocalBlockInbox, LocalBlockInboxReader) {
        let notifier = LocalBlockNotifier::new();
        let inbox = LocalBlockInbox(Rc::new(LocalInboxState::new(notifier)));
        let reader = LocalBlockInboxReader {
            inbox: inbox.clone(),
        };
        (inbox, reader)
    }

    /// Try to receive a queued block message without blocking.
    pub fn try_recv(&mut self) -> Option<BlockMessage> {
        self.inbox.try_recv()
    }

    /// Wait for the next queued block message.
    pub async fn recv(&mut self) -> Option<BlockMessage> {
        loop {
            if let Some(msg) = self.inbox.try_recv() {
                return Some(msg);
            }
            self.notified().await;
        }
    }

    /// Consume a pending message bit.
    pub fn take_message_pending(&self) -> bool {
        self.inbox.take_message_pending()
    }

    /// Consume a pending wakeup notification bit.
    pub fn take_pending(&self) -> bool {
        self.inbox.0.notifier.take_pending()
    }

    /// Future that resolves when the block is woken.
    pub fn notified(&self) -> LocalNotified {
        LocalNotified {
            state: self.inbox.0.notifier.clone(),
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
        let (_, rx) = LocalBlockInboxReader::pair();
        let n = rx.inbox.notifier();
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
        let (_, rx) = LocalBlockInboxReader::pair();
        let n = rx.inbox.notifier();
        n.notify();
        block_on(rx.notified());
        assert!(!rx.take_pending());
    }

    #[test]
    fn send_enqueues_and_wakes_reader() {
        let (tx, mut rx) = thread_safe_channel(1);

        block_on(tx.send(BlockMessage::Initialize)).unwrap();

        assert!(rx.take_pending());
        assert!(rx.take_message_pending());
        assert!(!rx.take_message_pending());
        assert!(matches!(rx.try_recv(), Some(BlockMessage::Initialize)));
    }

    #[test]
    fn local_send_enqueues_and_wakes_reader() {
        let (tx, mut rx) = LocalBlockInboxReader::pair();

        tx.push(BlockMessage::Initialize);

        assert!(rx.take_pending());
        assert!(rx.take_message_pending());
        assert!(!rx.take_message_pending());
        assert!(matches!(rx.try_recv(), Some(BlockMessage::Initialize)));
    }

    #[test]
    fn recv_waits_for_message() {
        let (tx, mut rx) = thread_safe_channel(1);

        block_on(tx.send(BlockMessage::Initialize)).unwrap();

        assert!(matches!(
            block_on(rx.recv()),
            Some(BlockMessage::Initialize)
        ));
    }

    #[test]
    fn notify_wakes_without_message() {
        let (tx, mut rx) = thread_safe_channel(1);

        tx.notify();

        assert!(rx.take_pending());
        assert!(!rx.take_message_pending());
        assert!(rx.try_recv().is_none());
    }

    #[test]
    fn local_notify_wakes_without_message() {
        let (tx, mut rx) = LocalBlockInboxReader::pair();

        tx.notify();

        assert!(rx.take_pending());
        assert!(!rx.take_message_pending());
        assert!(rx.try_recv().is_none());
    }

    #[test]
    fn multiple_sends_coalesce_but_keep_messages() {
        let (tx, mut rx) = thread_safe_channel(4);

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
