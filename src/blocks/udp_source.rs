use async_net::UdpSocket;
// use futures::AsyncReadExt;
//
use crate::anyhow::Context;
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

/// Read samples from a UDP socket.
pub struct UdpSource<T: Send + 'static> {
    bind: String,
    max_packet_bytes: usize,
    socket: Option<UdpSocket>,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> UdpSource<T> {
    /// Create UDP Source block
    pub fn new(bind: impl Into<String>, max_packet_bytes: usize) -> Block {
        Block::new(
            BlockMetaBuilder::new("UdpSource").build(),
            StreamIoBuilder::new().add_output::<T>("out").build(),
            MessageIoBuilder::new().build(),
            UdpSource {
                bind: bind.into(),
                max_packet_bytes,
                socket: None,
                _type: std::marker::PhantomData::<T>,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: Send + 'static> Kernel for UdpSource<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let out = sio.output(0).slice_unchecked::<u8>();
        if out.len() < self.max_packet_bytes {
            return Ok(());
        }

        match self
            .socket
            .as_ref()
            .context("no socket")?
            .recv_from(out)
            .await
        {
            Ok((s, _)) => {
                debug!("udp source read bytes {}", s);
                sio.output(0).produce(s / std::mem::size_of::<T>());
            }
            Err(_) => {
                debug!("udp source socket closed");
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
        self.socket = Some(UdpSocket::bind(self.bind.clone()).await?);
        Ok(())
    }
}
