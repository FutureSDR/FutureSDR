use anyhow::{Context, Result};
use async_net::{TcpListener, TcpStream};
use futures::AsyncReadExt;

use crate::runtime::AsyncKernel;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

pub struct TcpSource {
    port: u32,
    listener: Option<TcpListener>,
    socket: Option<TcpStream>,
}

impl TcpSource {
    pub fn new(port: u32) -> Block {
        Block::new_async(
            BlockMetaBuilder::new("TcpSource").build(),
            StreamIoBuilder::new().add_stream_output("out", 1).build(),
            MessageIoBuilder::new().build(),
            TcpSource {
                port,
                listener: None,
                socket: None,
            },
        )
    }
}

#[async_trait]
impl AsyncKernel for TcpSource {
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

pub struct TcpSourceBuilder {
    port: u32,
}

impl TcpSourceBuilder {
    pub fn new(port: u32) -> TcpSourceBuilder {
        TcpSourceBuilder { port }
    }

    pub fn build(self) -> Block {
        TcpSource::new(self.port)
    }
}
