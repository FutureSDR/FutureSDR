use async_net::UdpSocket;
use futures::FutureExt;

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

pub struct BlobToUdp {
    socket: Option<UdpSocket>,
    remote: String,
}

impl BlobToUdp {
    pub fn new<S>(remote: S) -> Block
    where
        S: Into<String>,
    {
        Block::new(
            BlockMetaBuilder::new("BlobToUdp").build(),
            StreamIoBuilder::new().build(),
            MessageIoBuilder::new()
                .add_input(
                    "in",
                    |block: &mut BlobToUdp,
                     _mio: &mut MessageIo<BlobToUdp>,
                     _meta: &mut BlockMeta,
                     p: Pmt| {
                        async move {
                            if let Pmt::Blob(v) = p {
                                block.socket.as_ref().unwrap().send(&v).await?;
                            }
                            Ok(Pmt::Null)
                        }
                        .boxed()
                    },
                )
                .build(),
            BlobToUdp {
                socket: None,
                remote: remote.into(),
            },
        )
    }
}

#[async_trait]
impl Kernel for BlobToUdp {
    async fn init(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let socket = UdpSocket::bind("127.0.0.1:0").await?;
        socket.connect(&self.remote).await?;
        self.socket = Some(socket);
        Ok(())
    }
}
