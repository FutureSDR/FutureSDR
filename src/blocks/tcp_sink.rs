use async_net::{TcpListener, TcpStream};
use futures::AsyncWriteExt;

use crate::anyhow::{bail, Context, Result};
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

/// Push samples into a TCP socket.
pub struct TcpSink {
    port: u32,
    listener: Option<TcpListener>,
    socket: Option<TcpStream>,
}

impl TcpSink {
    pub fn new(port: u32) -> Block {
        Block::new(
            BlockMetaBuilder::new("TcpSink").build(),
            StreamIoBuilder::new().add_input::<u8>("in").build(),
            MessageIoBuilder::new().build(),
            TcpSink {
                port,
                listener: None,
                socket: None,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for TcpSink {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        if self.socket.is_none() {
            let (socket, _) = self
                .listener
                .as_mut()
                .context("no listener")?
                .accept()
                .await?;
            self.socket = Some(socket);
            debug!("tcp sink accepted connection");
        }

        let i = sio.input(0).slice::<u8>();

        match self
            .socket
            .as_mut()
            .context("no socket")?
            .write_all(i)
            .await
        {
            Ok(()) => {}
            Err(_) => bail!("tcp sink socket error"),
        }

        if sio.input(0).finished() {
            io.finished = true;
        }

        debug!("tcp sink wrote bytes {}", i.len());
        sio.input(0).consume(i.len());

        Ok(())
    }

    async fn init(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        self.listener = Some(TcpListener::bind(format!("127.0.0.1:{}", self.port)).await?);
        Ok(())
    }
}
