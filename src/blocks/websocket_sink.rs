use async_io::Async;
use async_tungstenite::tungstenite::Message;
use async_tungstenite::WebSocketStream;
use futures::future;
use futures::future::Either;
use futures::sink::{Sink, SinkExt};
use futures::Stream;
use std::marker::PhantomData;
use std::mem::size_of;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use crate::anyhow::Context as _;
use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

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
pub struct WebsocketSink<T> {
    port: u32,
    listener: Option<Arc<Async<TcpListener>>>,
    conn: Option<WsStream>,
    mode: WebsocketSinkMode,
    _p: PhantomData<T>,
}

impl<T: Send + Sync + 'static> WebsocketSink<T> {
    pub fn new(port: u32, mode: WebsocketSinkMode) -> Block {
        Block::new(
            BlockMetaBuilder::new("WebsocketSink").build(),
            StreamIoBuilder::new()
                .add_input("in", size_of::<T>())
                .build(),
            MessageIoBuilder::<Self>::new().build(),
            WebsocketSink {
                port,
                listener: None,
                conn: None,
                mode,
                _p: PhantomData,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: Send + Sync + 'static> Kernel for WebsocketSink<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<u8>();
        debug_assert_eq!(i.len() % size_of::<T>(), 0);

        if sio.input(0).finished() {
            io.finished = true;
        }

        let item_size = size_of::<T>();
        let items = i.len() / item_size;

        if let Some(ref mut conn) = self.conn {
            if i.is_empty() {
                return Ok(());
            }

            let mut v = Vec::new();

            match &self.mode {
                WebsocketSinkMode::Blocking => {
                    v.extend_from_slice(i);
                    sio.input(0).consume(i.len() / size_of::<T>());
                }
                WebsocketSinkMode::FixedBlocking(block_size) => {
                    if *block_size <= items {
                        v.extend_from_slice(&i[0..(block_size * item_size)]);
                        sio.input(0).consume(*block_size);
                    }
                }
                WebsocketSinkMode::FixedDropping(block_size) => {
                    let n = items / block_size;
                    if n != 0 {
                        v.extend_from_slice(
                            &i[((n - 1) * block_size * item_size)..(n * block_size * item_size)],
                        );
                        sio.input(0).consume(n * block_size);
                    }
                }
            }

            if !v.is_empty() {
                let acc = Box::pin(self.listener.as_ref().context("no listener")?.accept());
                let send = conn.send(Message::Binary(v));

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
            .context("no listener")?
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
                let n = items / block_size;
                sio.input(0).consume(n * block_size);
            }

            let l = self.listener.as_ref().unwrap().clone();
            io.block_on(async move {
                l.readable().await.unwrap();
            });
        }

        Ok(())
    }

    async fn init(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
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
    pub fn new(port: u32) -> WebsocketSinkBuilder<T> {
        WebsocketSinkBuilder {
            port,
            mode: WebsocketSinkMode::Blocking,
            _p: PhantomData,
        }
    }

    #[must_use]
    pub fn mode(mut self, mode: WebsocketSinkMode) -> WebsocketSinkBuilder<T> {
        self.mode = mode;
        self
    }

    pub fn build(self) -> Block {
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
