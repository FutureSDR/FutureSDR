use async_net::{TcpListener, TcpStream};
use futures::AsyncReadExt;

use crate::anyhow::{Context, Result};
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
pub struct TcpSource {
    port: u32,
    listener: Option<TcpListener>,
    socket: Option<TcpStream>,
}

impl TcpSource {
    pub fn new(port: u32) -> Block {
        Block::new(
            BlockMetaBuilder::new("TcpSource").build(),
            StreamIoBuilder::new().add_output("out", 1).build(),
            MessageIoBuilder::new().build(),
            TcpSource {
                port,
                listener: None,
                socket: None,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for TcpSource {
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
            debug!("tcp source accepted connection");
        }

        let out = sio.output(0).slice::<u8>();
        if out.is_empty() {
            return Ok(());
        }

        match self
            .socket
            .as_mut()
            .context("no socket")?
            .read_exact(out)
            .await
        {
            Ok(_) => {
                debug!("tcp source read bytes {}", out.len());
                sio.output(0).produce(out.len());
            }
            Err(_) => {
                debug!("tcp source socket closed");
                io.finished = true;
            }
        }

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
