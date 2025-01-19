use anyhow::Context;
use async_net::TcpListener;
use async_net::TcpStream;
use futures::AsyncReadExt;

use crate::runtime::BlockMeta;
use crate::runtime::Kernel;
use crate::runtime::MessageOutputs;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Read samples from a TCP socket.
#[derive(Block)]
pub struct TcpSource<T: Send + 'static> {
    bind: String,
    listener: Option<TcpListener>,
    socket: Option<TcpStream>,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> TcpSource<T> {
    /// Create TCP Source block
    pub fn new(bind: impl Into<String>) -> TypedBlock<Self> {
        TypedBlock::new(
            StreamIoBuilder::new().add_output::<T>("out").build(),
            Self {
                bind: bind.into(),
                listener: None,
                socket: None,
                _type: std::marker::PhantomData::<T>,
            },
        )
    }
}

#[doc(hidden)]
impl<T: Send + 'static> Kernel for TcpSource<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageOutputs,
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

        let out = sio.output(0).slice_unchecked::<u8>();
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
                sio.output(0).produce(out.len() / std::mem::size_of::<T>());
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
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        self.listener = Some(TcpListener::bind(self.bind.clone()).await?);
        Ok(())
    }
}
