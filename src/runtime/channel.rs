//! Async channels used by the runtime and block implementation APIs.
//!
//! `mpsc` uses `kanal`, while `oneshot` wraps the channels from the `futures`
//! crate.

/// Multi-producer, single-consumer channels backed by `kanal`.
pub mod mpsc {
    use std::fmt;

    #[cfg(target_arch = "wasm32")]
    use crate::runtime::wasm_event_loop_yield;

    pub use ::kanal::ReceiveError;
    pub use ::kanal::SendError;

    /// Sending side of a channel.
    #[derive(Debug)]
    pub struct Sender<T>(::kanal::AsyncSender<T>);

    /// Receiving side of a channel.
    #[derive(Debug)]
    pub struct Receiver<T>(::kanal::AsyncReceiver<T>);

    /// Error returned by [`Receiver::try_recv`].
    #[derive(Debug, PartialEq, Eq)]
    pub enum TryRecvError {
        /// The channel is empty but still connected.
        Empty,
        /// The channel is disconnected.
        Disconnected,
    }

    /// Error returned by [`Sender::try_send`].
    #[derive(Debug, PartialEq, Eq)]
    pub enum TrySendError<T> {
        /// The channel is full.
        Full(T),
        /// The channel is disconnected.
        Disconnected(T),
    }

    impl fmt::Display for TryRecvError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Empty => write!(f, "receive failed because channel is empty"),
                Self::Disconnected => {
                    write!(f, "receive failed because sender dropped unexpectedly")
                }
            }
        }
    }

    impl std::error::Error for TryRecvError {}

    impl<T> fmt::Display for TrySendError<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Full(_) => write!(f, "send failed because channel is full"),
                Self::Disconnected(_) => {
                    write!(f, "send failed because receiver dropped unexpectedly")
                }
            }
        }
    }

    impl<T: fmt::Debug> std::error::Error for TrySendError<T> {}

    /// Create a bounded channel.
    pub fn channel<T>(size: usize) -> (Sender<T>, Receiver<T>) {
        let (tx, rx) = ::kanal::bounded_async(size);
        (Sender(tx), Receiver(rx))
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "mocker"))]
    pub(crate) fn unbounded<T>() -> (Sender<T>, Receiver<T>) {
        let (tx, rx) = ::kanal::unbounded_async();
        (Sender(tx), Receiver(rx))
    }

    impl<T> Clone for Sender<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl<T> Sender<T> {
        /// Send a value into the channel.
        #[cfg(not(target_arch = "wasm32"))]
        pub async fn send(&self, data: T) -> Result<(), SendError> {
            self.0.send(data).await
        }

        /// Send a value into the channel.
        #[cfg(target_arch = "wasm32")]
        pub async fn send(&self, mut data: T) -> Result<(), SendError> {
            loop {
                match self.try_send(data) {
                    Ok(()) => return Ok(()),
                    Err(TrySendError::Full(value)) => {
                        data = value;
                        wasm_event_loop_yield().await;
                    }
                    Err(TrySendError::Disconnected(_)) => return Err(SendError::ReceiveClosed),
                }
            }
        }

        /// Attempt to send a value without waiting.
        pub fn try_send(&self, data: T) -> Result<(), TrySendError<T>> {
            let mut data = Some(data);
            match self.0.try_send_option(&mut data) {
                Ok(true) => Ok(()),
                Ok(false) => Err(TrySendError::Full(data.expect("send data lost"))),
                Err(_) => Err(TrySendError::Disconnected(data.expect("send data lost"))),
            }
        }

        /// Return whether the receiver side has been closed.
        pub fn is_closed(&self) -> bool {
            self.0.is_disconnected() || self.0.is_closed()
        }

        /// Close the channel.
        pub fn close(&self) -> Result<(), SendError> {
            self.0.close().map_err(|_| SendError::Closed)
        }
    }

    impl<T> Receiver<T> {
        /// Close the channel.
        pub fn close(&self) -> Result<(), ReceiveError> {
            self.0.close().map_err(|_| ReceiveError::Closed)
        }

        /// Receive the next value from the channel.
        ///
        /// # Cancellation safety
        ///
        /// On native targets, this wraps `kanal`'s receive future. Once that
        /// future has been polled, it must be driven to completion unless this
        /// receiver will no longer be used. Do not put `recv()` directly in a
        /// losing `select` branch while continuing to use the channel; use
        /// [`Receiver::try_recv`] or keep the receive future pinned across
        /// losing branches instead.
        #[cfg(not(target_arch = "wasm32"))]
        pub async fn recv(&self) -> Option<T> {
            self.0.recv().await.ok()
        }

        /// Receive the next value from the channel.
        ///
        /// # Cancellation safety
        ///
        /// Keep portable code consistent with native behavior: do not put
        /// `recv()` directly in a losing `select` branch while continuing to
        /// use the channel. Use [`Receiver::try_recv`] or keep the receive
        /// future pinned across losing branches instead.
        #[cfg(target_arch = "wasm32")]
        pub async fn recv(&self) -> Option<T> {
            loop {
                match self.try_recv() {
                    Ok(value) => return Some(value),
                    Err(TryRecvError::Empty) => {
                        wasm_event_loop_yield().await;
                    }
                    Err(TryRecvError::Disconnected) => return None,
                }
            }
        }

        /// Attempt to receive a value without waiting.
        pub fn try_recv(&self) -> Result<T, TryRecvError> {
            match self.0.try_recv() {
                Ok(Some(v)) => Ok(v),
                Ok(None) => Err(TryRecvError::Empty),
                Err(_) => Err(TryRecvError::Disconnected),
            }
        }
    }
}

/// Single-use channels.
pub mod oneshot {
    use std::future::Future;
    use std::pin::Pin;
    use std::task::Context;
    use std::task::Poll;

    pub use futures::channel::oneshot::Canceled;

    /// Sending side of a oneshot channel.
    #[derive(Debug)]
    pub struct Sender<T>(futures::channel::oneshot::Sender<T>);

    /// Receiving side of a oneshot channel.
    #[derive(Debug)]
    pub struct Receiver<T> {
        inner: futures::channel::oneshot::Receiver<T>,
        #[cfg(target_arch = "wasm32")]
        wake_scheduled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    /// Create a oneshot channel.
    pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
        let (tx, rx) = futures::channel::oneshot::channel();
        (
            Sender(tx),
            Receiver {
                inner: rx,
                #[cfg(target_arch = "wasm32")]
                wake_scheduled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            },
        )
    }

    impl<T> Sender<T> {
        /// Send a value to the receiver.
        pub fn send(self, data: T) -> Result<(), T> {
            self.0.send(data)
        }
    }

    impl<T> Future for Receiver<T> {
        type Output = Result<T, Canceled>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            #[cfg(not(target_arch = "wasm32"))]
            {
                Pin::new(&mut self.inner).poll(cx)
            }

            #[cfg(target_arch = "wasm32")]
            {
                // Do not register `cx.waker()` with the underlying futures
                // oneshot on WASM: the sender may live in another web worker,
                // and waking a wasm-bindgen/async-task waker from there can
                // reschedule the receiving future on the wrong worker. Poll the
                // inner channel with a noop waker and schedule a local timer to
                // check again on the receiver's own event loop.
                let mut noop_cx = Context::from_waker(futures::task::noop_waker_ref());
                match Pin::new(&mut self.inner).poll(&mut noop_cx) {
                    Poll::Ready(result) => Poll::Ready(result),
                    Poll::Pending => {
                        schedule_wake(&self.wake_scheduled, cx);
                        Poll::Pending
                    }
                }
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn schedule_wake(
        wake_scheduled: &std::sync::Arc<std::sync::atomic::AtomicBool>,
        cx: &Context<'_>,
    ) {
        use std::sync::atomic::Ordering;

        if !wake_scheduled.swap(true, Ordering::AcqRel) {
            let wake_scheduled_for_callback = wake_scheduled.clone();
            let waker = cx.waker().clone();
            let callback = wasm_bindgen::closure::Closure::once_into_js(move || {
                wake_scheduled_for_callback.store(false, Ordering::Release);
                waker.wake();
            });
            let function = wasm_bindgen::JsCast::unchecked_ref::<js_sys::Function>(&callback);
            if let Some(window) = web_sys::window() {
                let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(function, 1);
            } else if let Ok(scope) =
                wasm_bindgen::JsCast::dyn_into::<web_sys::WorkerGlobalScope>(js_sys::global())
            {
                let _ = scope.set_timeout_with_callback_and_timeout_and_arguments_0(function, 1);
            } else {
                wake_scheduled.store(false, Ordering::Release);
                cx.waker().wake_by_ref();
            }
        }
    }
}
