use anyhow::Context;
use async_net::UdpSocket;

use crate::runtime::BlockMeta;
use crate::runtime::Kernel;
use crate::runtime::MessageOutputs;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Read samples from a UDP socket.
#[derive(Block)]
pub struct UdpSource<T: Send + 'static> {
    bind: String,
    max_packet_bytes: usize,
    socket: Option<UdpSocket>,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> UdpSource<T> {
    /// Create UDP Source block
    pub fn new(bind: impl Into<String>, max_packet_bytes: usize) -> TypedBlock<Self> {
        TypedBlock::new(
            StreamIoBuilder::new().add_output::<T>("out").build(),
            Self {
                bind: bind.into(),
                max_packet_bytes,
                socket: None,
                _type: std::marker::PhantomData::<T>,
            },
        )
    }
}

#[doc(hidden)]
impl<T: Send + 'static> Kernel for UdpSource<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageOutputs,
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
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        self.socket = Some(UdpSocket::bind(self.bind.clone()).await?);
        Ok(())
    }
}
