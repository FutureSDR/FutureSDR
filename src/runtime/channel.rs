//! Async channels used by the runtime and block implementation APIs.
//!
//! `mpsc` uses `kanal`, while `oneshot` uses the channels from the `futures`
//! crate.

/// Multi-producer, single-consumer channels backed by `kanal`.
pub mod mpsc {
    use std::fmt;
    use std::sync::Arc;

    pub use ::kanal::ReceiveError;
    pub use ::kanal::SendError;

    /// Sending side of a channel.
    #[derive(Debug)]
    pub struct Sender<T>(Arc<::kanal::AsyncSender<T>>);

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
        (Sender(Arc::new(tx)), Receiver(rx))
    }

    impl<T> Clone for Sender<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl<T> Sender<T> {
        /// Send a value into the channel.
        pub async fn send(&self, data: T) -> Result<(), SendError> {
            self.0.send(data).await
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
        pub async fn close(&self) -> Result<(), SendError> {
            self.0.close().map_err(|_| SendError::Closed)
        }

        /// Return whether two senders point to the same receiver.
        pub fn same_receiver(&self, other: &Self) -> bool {
            Arc::ptr_eq(&self.0, &other.0)
        }
    }

    impl<T> Receiver<T> {
        /// Receive the next value from the channel.
        pub async fn recv(&self) -> Option<T> {
            self.0.recv().await.ok()
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

pub use futures::channel::oneshot;
