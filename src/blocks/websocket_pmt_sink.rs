use async_io::Async;
use async_tungstenite::tungstenite::Message;
use async_tungstenite::WebSocketStream;
use futures::future;
use futures::future::Either;
use futures::sink::Sink;
use futures::sink::SinkExt;
use futures::Stream;
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::net::TcpListener;
use std::net::TcpStream;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;

use crate::anyhow::Context as _;
use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Pmt;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

/// Push Samples from PMTs in a WebSocket.
pub struct WebsocketPmtSink {
    port: u32,
    listener: Option<Arc<Async<TcpListener>>>,
    conn: Option<WsStream>,
    pmts: VecDeque<Pmt>,
}

impl WebsocketPmtSink {
    /// Create WebsocketPmtSink block
    pub fn new(port: u32) -> Block {
        Block::new(
            BlockMetaBuilder::new("WebsocketPmtSink").build(),
            StreamIoBuilder::new().build(),
            MessageIoBuilder::<Self>::new()
                .add_input("in", Self::handler)
                .build(),
            WebsocketPmtSink {
                port,
                listener: None,
                conn: None,
                pmts: VecDeque::new(),
            },
        )
    }

    #[message_handler]
    async fn handler(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        match p {
            Pmt::Finished => {
                io.finished = true;
            }
            _ => {
                self.pmts.push_back(p);
            }
        }
        Ok(Pmt::Ok)
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for WebsocketPmtSink {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        if let Some(ref mut conn) = self.conn {
            let msg = match self.pmts.pop_front() {
                Some(Pmt::VecCF32(v)) => {
                    let v: Vec<u8> = v
                        .into_iter()
                        .flat_map(|f| {
                            let mut b = [0; 8];
                            b[..4].copy_from_slice(&f.re.to_le_bytes());
                            b[4..].copy_from_slice(&f.im.to_le_bytes());
                            b
                        })
                        .collect();
                    if !v.is_empty() {
                        Some(Message::Binary(v))
                    } else {
                        None
                    }
                }
                Some(Pmt::VecF32(v)) => {
                    let v: Vec<u8> = v
                        .into_iter()
                        .flat_map(|f| {
                            let mut b = [0; 4];
                            b.copy_from_slice(&f.to_le_bytes());
                            b
                        })
                        .collect();
                    if !v.is_empty() {
                        Some(Message::Binary(v))
                    } else {
                        None
                    }
                }
                Some(Pmt::VecU64(v)) => {
                    let v: Vec<u8> = v
                        .into_iter()
                        .flat_map(|f| {
                            let mut b = [0; 8];
                            b.copy_from_slice(&f.to_le_bytes());
                            b
                        })
                        .collect();
                    if !v.is_empty() {
                        Some(Message::Binary(v))
                    } else {
                        None
                    }
                }
                Some(Pmt::Blob(b)) => Some(Message::Binary(b)),
                Some(Pmt::String(s)) => Some(Message::Text(s)),
                Some(p) => {
                    warn!("WebsocketPmtSink: unsupported PMT type {:?}", p);
                    None
                }
                None => None,
            };

            if let Some(msg) = msg {
                let acc = Box::pin(self.listener.as_ref().context("no listener")?.accept());
                let send = conn.send(msg);

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

            if !self.pmts.is_empty() {
                io.call_again = true;
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
