use anyhow::bail;
use anyhow::Context;
use async_net::TcpListener;
use async_net::TcpStream;
use futures::AsyncWriteExt;

use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Push samples into a TCP socket.
pub struct TcpSink<T: Send + 'static> {
    port: u32,
    listener: Option<TcpListener>,
    socket: Option<TcpStream>,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> TcpSink<T> {
    /// Create TCP Sink block
    pub fn new(port: u32) -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("TcpSink").build(),
            StreamIoBuilder::new().add_input::<T>("in").build(),
            MessageIoBuilder::new().build(),
            TcpSink {
                port,
                listener: None,
                socket: None,
                _type: std::marker::PhantomData::<T>,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: Send + 'static> Kernel for TcpSink<T> {
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

        let i = sio.input(0).slice_unchecked::<u8>();

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

        debug!(
            "tcp sink wrote bytes {}",
            i.len() / std::mem::size_of::<T>()
        );
        sio.input(0).consume(i.len() / std::mem::size_of::<T>());

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
