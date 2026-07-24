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
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::PortIndex;
use crate::runtime::channel::mpsc;
use crate::runtime::config::config;
use crate::runtime::local_domain::LocalDomainInbox;

static NEXT_LOCAL_DOMAIN_KEY: AtomicUsize = AtomicUsize::new(0);

/// Runtime-unique identity for one local-domain execution resource.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) struct LocalDomainKey(usize);

impl LocalDomainKey {
    pub(crate) fn new() -> Self {
        Self(NEXT_LOCAL_DOMAIN_KEY.fetch_add(1, Ordering::Relaxed))
    }
}

/// Direct address of a block inside one local-domain state.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) struct LocalBlockAddr {
    pub(crate) block_id: BlockId,
    pub(crate) local_id: usize,
}

impl LocalBlockAddr {
    pub(crate) fn new(block_id: BlockId, local_id: usize) -> Self {
        Self { block_id, local_id }
    }
}

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
    pub(crate) fn take_pending(&self) -> bool {
        self.state.pending.swap(false, Ordering::AcqRel)
    }

    /// Return a future that resolves on the next pending notification.
    pub(crate) fn notified(&self) -> Notified {
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

/// Future returned by [`BlockNotifier::notified`].
///
/// Polling this future consumes one pending notification bit.
pub(crate) struct Notified {
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

/// Buffer-facing send-capable inbox handle for a normal-domain block.
///
/// Buffer implementations use this handle to wake a block or report stream
/// finish notifications. Runtime/control/message ingress uses the
/// runtime-internal `BlockEndpoint` routing handle instead.
#[derive(Clone, Debug)]
pub struct BlockInbox {
    tx: mpsc::Sender<BlockMessage>,
    notifier: BlockNotifier,
}

/// Runtime-internal send-safe endpoint for routing messages to a block.
///
/// Normal-domain blocks use a direct concrete inbox internally, while
/// local-domain blocks use a domain proxy that forwards into the private local
/// inbox owned by the domain. Buffer implementations receive [`BlockInbox`] or
/// [`LocalBlockInbox`] instead of this erased routing endpoint.
#[derive(Clone)]
pub(crate) enum BlockEndpoint {
    Direct(BlockInbox),
    DomainProxy {
        domain: LocalDomainInbox,
        addr: LocalBlockAddr,
    },
}

impl fmt::Debug for BlockEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlockEndpoint::Direct(_) => f
                .debug_struct("BlockEndpoint::Direct")
                .finish_non_exhaustive(),
            BlockEndpoint::DomainProxy { addr, .. } => f
                .debug_struct("BlockEndpoint::DomainProxy")
                .field("addr", addr)
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

    /// Create a paired concrete sender/reader block inbox with a coalescing notifier.
    pub(crate) fn pair(size: usize) -> (BlockInbox, BlockInboxReader) {
        let (control, receiver) = mpsc::channel::<BlockMessage>(size);
        let notifier = BlockNotifier::new();
        (
            BlockInbox::new(control, notifier.clone()),
            BlockInboxReader::new(receiver, notifier),
        )
    }

    /// Get a wake-only notifier for the destination block.
    #[inline(always)]
    pub fn notifier(&self) -> BlockNotifier {
        self.notifier.clone()
    }

    /// Return whether the underlying receiver has been closed.
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }

    /// Wake the destination block without sending a message.
    #[inline(always)]
    pub fn notify(&self) {
        self.notifier.notify();
    }

    /// Notify the destination block that one stream input port is done.
    pub async fn stream_input_done(&self, input_id: PortIndex) -> Result<(), Error> {
        self.send(BlockMessage::StreamInputDone { input_id }).await
    }

    /// Notify the destination block that one stream output port is done.
    pub async fn stream_output_done(&self, output_id: PortIndex) -> Result<(), Error> {
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

impl Default for BlockNotifier {
    fn default() -> Self {
        Self::new()
    }
}

impl From<BlockInbox> for BlockEndpoint {
    fn from(inbox: BlockInbox) -> Self {
        Self::Direct(inbox)
    }
}

impl BlockEndpoint {
    /// Create a sender-side domain proxy for a local-domain block.
    pub(crate) fn domain_proxy(domain: LocalDomainInbox, addr: LocalBlockAddr) -> Self {
        Self::DomainProxy { domain, addr }
    }

    /// Enqueue a block message and wake the destination block on success.
    pub(crate) async fn send(&self, msg: BlockMessage) -> Result<(), Error> {
        match self {
            BlockEndpoint::Direct(inbox) => inbox.send(msg).await,
            BlockEndpoint::DomainProxy { domain, addr } => {
                if has_current_local_inbox(domain.key(), *addr) {
                    return LocalSend {
                        target: CurrentLocalInbox {
                            key: domain.key(),
                            addr: *addr,
                        },
                        msg: Some(msg),
                    }
                    .await;
                }

                match msg {
                    BlockMessage::Call { port_id, data, tx } => {
                        domain.call(*addr, port_id, data, tx).await
                    }
                    msg => domain.post(*addr, msg).await,
                }
            }
        }
    }
}

/// Runtime-internal receiver-side inbox for normal thread-safe blocks.
#[derive(Debug)]
pub(crate) struct BlockInboxReader {
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
    pub(crate) fn take_message_pending(&self) -> bool {
        self.notifier.take_message_pending()
    }

    /// Consume a pending wakeup notification bit.
    pub(crate) fn take_pending(&self) -> bool {
        self.notifier.take_pending()
    }

    /// Future that resolves when the block is woken.
    pub(crate) fn notified(&self) -> Notified {
        self.notifier.notified()
    }
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
    /// Create a new local-domain notifier.
    pub fn new() -> Self {
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
    capacity: usize,
    send_wakers: RefCell<Vec<Waker>>,
    closed: Cell<bool>,
    message_pending: Cell<bool>,
    notifier: LocalBlockNotifier,
}

/// Buffer-facing inbox handle for blocks running inside one local domain.
#[derive(Clone, Debug)]
pub struct LocalBlockInbox(Rc<LocalInboxState>);

impl LocalInboxState {
    fn new(notifier: LocalBlockNotifier) -> Self {
        let queue = VecDeque::with_capacity(config().queue_size);
        let capacity = queue.capacity();
        Self {
            queue: RefCell::new(queue),
            capacity,
            send_wakers: RefCell::new(Vec::new()),
            closed: Cell::new(false),
            message_pending: Cell::new(false),
            notifier,
        }
    }

    fn wake_senders(&self) {
        for waker in self.send_wakers.borrow_mut().drain(..) {
            waker.wake();
        }
    }

    fn register_sender(&self, waker: &Waker) {
        let mut send_wakers = self.send_wakers.borrow_mut();
        if !send_wakers
            .iter()
            .any(|registered| registered.will_wake(waker))
        {
            send_wakers.push(waker.clone());
        }
    }

    fn close(&self) {
        if self.closed.replace(true) {
            return;
        }

        self.message_pending.set(false);
        let messages = self.queue.borrow_mut().drain(..).collect::<Vec<_>>();
        self.wake_senders();

        for msg in messages {
            fail_closed_message(msg);
        }
    }
}

impl Default for LocalBlockNotifier {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalBlockInbox {
    pub(crate) fn notify(&self) {
        self.0.notifier.notify();
    }

    fn try_send(&self, msg: BlockMessage) -> Result<(), LocalTrySendError> {
        if self.0.closed.get() {
            return Err(LocalTrySendError::Closed(msg));
        }

        let mut queue = self.0.queue.borrow_mut();
        if queue.len() == self.0.capacity {
            return Err(LocalTrySendError::Full(msg));
        }

        queue.push_back(msg);
        self.0.message_pending.set(true);
        self.notify();
        Ok(())
    }

    pub(crate) async fn send(&self, msg: BlockMessage) -> Result<(), Error> {
        LocalSend {
            target: self.clone(),
            msg: Some(msg),
        }
        .await
    }

    /// Get a wake-only notifier for this local block.
    pub fn notifier(&self) -> LocalBlockNotifier {
        self.0.notifier.clone()
    }

    fn try_recv(&self) -> Option<BlockMessage> {
        let msg = self.0.queue.borrow_mut().pop_front();
        if msg.is_some() {
            self.0.wake_senders();
        }
        msg
    }

    fn take_message_pending(&self) -> bool {
        self.0.message_pending.replace(false)
    }
}

enum LocalTrySendError {
    Full(BlockMessage),
    Closed(BlockMessage),
}

fn fail_closed_message(msg: BlockMessage) -> Error {
    if let BlockMessage::Call { tx, .. } = msg {
        let _ = tx.send(Err(Error::BlockTerminated));
    }
    Error::BlockTerminated
}

#[derive(Debug)]
struct CurrentLocalDomain {
    key: LocalDomainKey,
    inboxes: Vec<Option<(BlockId, LocalBlockInbox)>>,
}

thread_local! {
    static CURRENT_LOCAL_DOMAIN: RefCell<Option<CurrentLocalDomain>> = const { RefCell::new(None) };
}

/// Guard for a same-thread local-domain fast-path context.
pub(crate) struct LocalDomainContextGuard {
    previous: Option<CurrentLocalDomain>,
}

impl Drop for LocalDomainContextGuard {
    fn drop(&mut self) {
        CURRENT_LOCAL_DOMAIN.with(|current| {
            current.replace(self.previous.take());
        });
    }
}

/// Install the current local-domain context for tasks polled on this thread.
pub(crate) fn enter_local_domain_context(
    key: LocalDomainKey,
    inboxes: Vec<Option<(BlockId, LocalBlockInbox)>>,
) -> LocalDomainContextGuard {
    CURRENT_LOCAL_DOMAIN.with(|current| LocalDomainContextGuard {
        previous: current.replace(Some(CurrentLocalDomain { key, inboxes })),
    })
}

fn current_local_inbox(key: LocalDomainKey, addr: LocalBlockAddr) -> Option<LocalBlockInbox> {
    CURRENT_LOCAL_DOMAIN.with(|current| {
        let current = current.borrow();
        let current = current.as_ref()?;
        if current.key != key {
            return None;
        }

        let (block_id, inbox) = current.inboxes.get(addr.local_id)?.as_ref()?;
        (*block_id == addr.block_id).then(|| inbox.clone())
    })
}

fn has_current_local_inbox(key: LocalDomainKey, addr: LocalBlockAddr) -> bool {
    CURRENT_LOCAL_DOMAIN.with(|current| {
        let current = current.borrow();
        current.as_ref().is_some_and(|current| {
            if current.key != key {
                return false;
            }

            current
                .inboxes
                .get(addr.local_id)
                .and_then(Option::as_ref)
                .is_some_and(|(block_id, _)| *block_id == addr.block_id)
        })
    })
}

// Keep the local send state machine shared while preserving sendability for
// `BlockEndpoint::send`: the domain-proxy fast path uses `CurrentLocalInbox`,
// which does not store the non-Send `LocalBlockInbox` across await points.
trait LocalSendTarget {
    fn inbox(&self) -> Result<LocalBlockInbox, Error>;
}

impl LocalSendTarget for LocalBlockInbox {
    fn inbox(&self) -> Result<LocalBlockInbox, Error> {
        Ok(self.clone())
    }
}

struct CurrentLocalInbox {
    key: LocalDomainKey,
    addr: LocalBlockAddr,
}

impl LocalSendTarget for CurrentLocalInbox {
    fn inbox(&self) -> Result<LocalBlockInbox, Error> {
        current_local_inbox(self.key, self.addr).ok_or_else(|| {
            Error::RuntimeError(
                "local-domain fast path polled outside its domain context".to_string(),
            )
        })
    }
}

struct LocalSend<T> {
    target: T,
    msg: Option<BlockMessage>,
}

impl<T: Unpin> Unpin for LocalSend<T> {}

impl<T: LocalSendTarget + Unpin> Future for LocalSend<T> {
    type Output = Result<(), Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let Some(msg) = this.msg.take() else {
            return Poll::Ready(Ok(()));
        };

        let inbox = match this.target.inbox() {
            Ok(inbox) => inbox,
            Err(error) => return Poll::Ready(Err(error)),
        };

        match inbox.try_send(msg) {
            Ok(()) => Poll::Ready(Ok(())),
            Err(LocalTrySendError::Closed(msg)) => Poll::Ready(Err(fail_closed_message(msg))),
            Err(LocalTrySendError::Full(msg)) => {
                this.msg = Some(msg);
                inbox.0.register_sender(cx.waker());

                let msg = this.msg.take().expect("local send message missing");
                let inbox = match this.target.inbox() {
                    Ok(inbox) => inbox,
                    Err(error) => return Poll::Ready(Err(error)),
                };
                match inbox.try_send(msg) {
                    Ok(()) => Poll::Ready(Ok(())),
                    Err(LocalTrySendError::Closed(msg)) => {
                        Poll::Ready(Err(fail_closed_message(msg)))
                    }
                    Err(LocalTrySendError::Full(msg)) => {
                        this.msg = Some(msg);
                        Poll::Pending
                    }
                }
            }
        }
    }
}

/// Receiver-side inbox for local-domain blocks.
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
    pub(crate) fn try_recv(&mut self) -> Option<BlockMessage> {
        self.inbox.try_recv()
    }

    /// Wait for the next queued block message.
    pub(crate) async fn recv(&mut self) -> Option<BlockMessage> {
        loop {
            if let Some(msg) = self.inbox.try_recv() {
                return Some(msg);
            }
            if self.inbox.0.closed.get() {
                return None;
            }
            self.notified().await;
        }
    }

    /// Consume a pending message bit.
    pub(crate) fn take_message_pending(&self) -> bool {
        self.inbox.take_message_pending()
    }

    /// Consume a pending wakeup notification bit.
    pub(crate) fn take_pending(&self) -> bool {
        self.inbox.0.notifier.take_pending()
    }

    /// Future that resolves when the block is woken.
    pub(crate) fn notified(&self) -> LocalNotified {
        LocalNotified {
            state: self.inbox.0.notifier.clone(),
        }
    }
}

impl Drop for LocalBlockInboxReader {
    fn drop(&mut self) {
        self.inbox.0.close();
    }
}

/// Future returned by [`LocalBlockInboxReader::notified`].
pub(crate) struct LocalNotified {
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
    use crate::runtime::Pmt;
    use crate::runtime::PortIndex;
    use crate::runtime::channel::oneshot;
    use futures::executor::block_on;
    use std::task::Context;
    use std::task::Poll;

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
        let (tx, mut rx) = BlockInbox::pair(1);

        block_on(tx.send(BlockMessage::Initialize)).unwrap();

        assert!(rx.take_pending());
        assert!(rx.take_message_pending());
        assert!(!rx.take_message_pending());
        assert!(matches!(rx.try_recv(), Some(BlockMessage::Initialize)));
    }

    #[test]
    fn local_send_enqueues_and_wakes_reader() {
        let (tx, mut rx) = LocalBlockInboxReader::pair();

        block_on(tx.send(BlockMessage::Initialize)).unwrap();

        assert!(rx.take_pending());
        assert!(rx.take_message_pending());
        assert!(!rx.take_message_pending());
        assert!(matches!(rx.try_recv(), Some(BlockMessage::Initialize)));
    }

    #[test]
    fn local_queue_is_bounded_by_allocated_capacity() {
        let (tx, mut rx) = LocalBlockInboxReader::pair();
        let capacity = tx.0.capacity;

        for _ in 0..capacity {
            assert!(tx.try_send(BlockMessage::Terminate).is_ok());
        }

        assert!(matches!(
            tx.try_send(BlockMessage::Initialize),
            Err(LocalTrySendError::Full(BlockMessage::Initialize))
        ));

        for _ in 0..capacity {
            assert!(matches!(rx.try_recv(), Some(BlockMessage::Terminate)));
        }
        assert!(rx.try_recv().is_none());
    }

    #[test]
    fn local_send_after_reader_drop_fails() {
        let (tx, rx) = LocalBlockInboxReader::pair();
        drop(rx);

        assert_eq!(
            block_on(tx.send(BlockMessage::Initialize)),
            Err(Error::BlockTerminated)
        );
    }

    #[test]
    fn local_call_after_reader_drop_replies_block_terminated() {
        let (tx, rx) = LocalBlockInboxReader::pair();
        let (reply_tx, reply_rx) = oneshot::channel();
        drop(rx);

        assert_eq!(
            block_on(tx.send(BlockMessage::Call {
                port_id: PortIndex::new(0),
                data: Pmt::Null,
                tx: reply_tx,
            })),
            Err(Error::BlockTerminated)
        );
        assert_eq!(block_on(reply_rx).unwrap(), Err(Error::BlockTerminated));
    }

    #[test]
    fn local_queued_call_replies_block_terminated_on_reader_drop() {
        let (tx, rx) = LocalBlockInboxReader::pair();
        let (reply_tx, reply_rx) = oneshot::channel();

        assert!(matches!(
            tx.try_send(BlockMessage::Call {
                port_id: PortIndex::new(0),
                data: Pmt::Null,
                tx: reply_tx,
            }),
            Ok(())
        ));
        drop(rx);

        assert_eq!(block_on(reply_rx).unwrap(), Err(Error::BlockTerminated));
    }

    #[test]
    fn local_pending_send_wakes_and_fails_on_reader_drop() {
        let (tx, rx) = LocalBlockInboxReader::pair();

        for _ in 0..tx.0.capacity {
            assert!(tx.try_send(BlockMessage::Terminate).is_ok());
        }

        let send = tx.send(BlockMessage::Initialize);
        futures::pin_mut!(send);
        let mut cx = Context::from_waker(futures::task::noop_waker_ref());
        assert!(matches!(
            Future::poll(send.as_mut(), &mut cx),
            Poll::Pending
        ));

        drop(rx);

        assert!(matches!(
            Future::poll(send.as_mut(), &mut cx),
            Poll::Ready(Err(Error::BlockTerminated))
        ));
    }

    #[test]
    fn recv_waits_for_message() {
        let (tx, mut rx) = BlockInbox::pair(1);

        block_on(tx.send(BlockMessage::Initialize)).unwrap();

        assert!(matches!(
            block_on(rx.recv()),
            Some(BlockMessage::Initialize)
        ));
    }

    #[test]
    fn notify_wakes_without_message() {
        let (tx, mut rx) = BlockInbox::pair(1);

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
    fn current_local_inbox_uses_local_slot_and_validates_block_id() {
        let key = LocalDomainKey::new();
        let (tx, _rx) = LocalBlockInboxReader::pair();
        let _guard = enter_local_domain_context(key, vec![None, Some((BlockId(7), tx.clone()))]);

        assert!(current_local_inbox(key, LocalBlockAddr::new(BlockId(7), 1)).is_some());
        assert!(has_current_local_inbox(
            key,
            LocalBlockAddr::new(BlockId(7), 1)
        ));
        assert!(current_local_inbox(key, LocalBlockAddr::new(BlockId(8), 1)).is_none());
        assert!(current_local_inbox(key, LocalBlockAddr::new(BlockId(7), 0)).is_none());
        assert!(current_local_inbox(key, LocalBlockAddr::new(BlockId(7), 2)).is_none());
    }

    #[test]
    fn multiple_sends_coalesce_but_keep_messages() {
        let (tx, mut rx) = BlockInbox::pair(4);

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
