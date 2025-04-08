use async_io::Async;
use async_tungstenite::tungstenite::Message;
use async_tungstenite::WebSocketStream;
use futures::future;
use futures::future::Either;
use futures::sink::Sink;
use futures::sink::SinkExt;
use futures::Stream;
use std::marker::PhantomData;
use std::mem::size_of;
use std::net::SocketAddr;
use std::net::TcpListener;
use std::net::TcpStream;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;

use crate::prelude::*;

/// Operation mode for [WebsocketSink].
pub enum WebsocketSinkMode {
    /// Backpressure. Block until all samples are sent.
    Blocking,
    /// Sent samples in fixed-size chunks. Block until all samples are sent.
    FixedBlocking(usize),
    /// Sent samples in fixed-size chunks. Drop first chunks if multiple are available in input buffer.
    FixedDropping(usize),
}

/// Push samples in a WebSocket.
#[derive(Block)]
pub struct WebsocketSink<T: Send, I: CpuBufferReader<Item = T> = circular::Reader<T>> {
    #[input]
    input: I,
    port: u32,
    listener: Option<Arc<Async<TcpListener>>>,
    conn: Option<WsStream>,
    mode: WebsocketSinkMode,
    _p: PhantomData<T>,
}

impl<T, I> WebsocketSink<T, I>
where
    T: Send + Sync + 'static,
    I: CpuBufferReader<Item = T>,
{
    /// Create WebsocketSink block
    pub fn new(port: u32, mode: WebsocketSinkMode) -> Self {
        Self {
            input: I::default(),
            port,
            listener: None,
            conn: None,
            mode,
            _p: PhantomData,
        }
    }
}

#[doc(hidden)]
impl<T, I> Kernel for WebsocketSink<T, I>
where
    T: Clone + Send + Sync + 'static,
    I: CpuBufferReader<Item = T>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        if self.input.finished() {
            io.finished = true;
        }

        let i = self.input.slice();
        let i_len = i.len();

        if let Some(ref mut conn) = self.conn {
            if i.is_empty() {
                return Ok(());
            }

            let mut v = Vec::new();

            match &self.mode {
                WebsocketSinkMode::Blocking => {
                    v.extend_from_slice(i);
                    self.input.consume(i_len);
                }
                WebsocketSinkMode::FixedBlocking(block_size) => {
                    if *block_size <= i_len {
                        v.extend_from_slice(&i[0..*block_size]);
                        self.input.consume(*block_size);
                    }
                }
                WebsocketSinkMode::FixedDropping(block_size) => {
                    let n = i_len / block_size;
                    if n != 0 {
                        v.extend_from_slice(&i[((n - 1) * block_size)..(n * block_size)]);
                        self.input.consume(n * block_size);
                    }
                }
            }

            if !v.is_empty() {
                let acc = Box::pin(
                    self.listener
                        .as_ref()
                        .ok_or_else(|| Error::RuntimeError("no listener".to_string()))?
                        .accept(),
                );

                let len = v.len() * size_of::<T>();
                let cap = v.capacity() * size_of::<T>();
                let ptr = v.as_ptr() as *mut u8;

                // prevent original Vec from dropping
                std::mem::forget(v);

                let v = unsafe { Vec::from_raw_parts(ptr, len, cap) };
                let send = conn.send(Message::Binary(v.into()));

                match future::select(acc, send).await {
                    Either::Left((a, _)) => {
                        if let Ok((stream, _)) = a {
                            self.conn = Some(WsStream {
                                inner: async_tungstenite::accept_async(stream).await?,
                            });
                        }
                    }
                    Either::Right((s, _)) => {
                        if s.is_err() {
                            debug!("websocket: client disconnected");
                            self.conn = None;
                        }
                    }
                }
            }
        } else if let Ok((stream, socket)) = self
            .listener
            .as_ref()
            .ok_or_else(|| Error::RuntimeError("no listener".to_string()))?
            .get_ref()
            .accept()
        {
            debug!("Websocket Accepted client: {}", socket);
            self.conn = Some(WsStream {
                inner: async_tungstenite::accept_async(Async::new(stream)?).await?,
            });
            io.call_again = true;
        } else {
            if let WebsocketSinkMode::FixedDropping(block_size) = &self.mode {
                let n = i_len / block_size;
                self.input.consume(n * block_size);
            }

            let l = self.listener.as_ref().unwrap().clone();
            io.block_on(async move {
                l.readable().await.unwrap();
            });
        }

        Ok(())
    }

    async fn init(&mut self, _mio: &mut MessageOutputs, _meta: &mut BlockMeta) -> Result<()> {
        self.listener = Some(Arc::new(Async::<TcpListener>::bind(
            format!("0.0.0.0:{}", self.port).parse::<SocketAddr>()?,
        )?));
        Ok(())
    }
}

/// Build a [WebsocketSink].
pub struct WebsocketSinkBuilder<T> {
    port: u32,
    mode: WebsocketSinkMode,
    _p: PhantomData<T>,
}

impl<T: Send + Sync + 'static> WebsocketSinkBuilder<T> {
    /// Create WebsocketSink builder
    pub fn new(port: u32) -> WebsocketSinkBuilder<T> {
        WebsocketSinkBuilder {
            port,
            mode: WebsocketSinkMode::Blocking,
            _p: PhantomData,
        }
    }

    /// Set mode
    #[must_use]
    pub fn mode(mut self, mode: WebsocketSinkMode) -> WebsocketSinkBuilder<T> {
        self.mode = mode;
        self
    }

    /// Build WebsocketSink
    pub fn build(self) -> WebsocketSink<T> {
        WebsocketSink::<T>::new(self.port, self.mode)
    }
}

struct WsStream {
    inner: WebSocketStream<Async<TcpStream>>,
}

impl Sink<Message> for WsStream {
    type Error = async_tungstenite::tungstenite::Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.inner).poll_ready(cx)
    }

    fn start_send(mut self: Pin<&mut Self>, item: Message) -> Result<(), Self::Error> {
        Pin::new(&mut self.inner).start_send(item)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.inner).poll_close(cx)
    }
}

impl Stream for WsStream {
    type Item = async_tungstenite::tungstenite::Result<Message>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}
