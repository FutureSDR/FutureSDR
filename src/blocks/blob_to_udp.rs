use std::net::ToSocketAddrs;

use async_net::SocketAddr;
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

/// Push [Blobs](crate::runtime::Pmt::Blob) into a UDP socket.
pub struct BlobToUdp {
    socket: Option<UdpSocket>,
    remote: SocketAddr,
}

impl BlobToUdp {
    pub fn new<S>(remote: S) -> Block
    where
        S: AsRef<str>,
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
                                match block
                                    .socket
                                    .as_ref()
                                    .unwrap()
                                    .send_to(&v, block.remote)
                                    .await
                                {
                                    Ok(s) => {
                                        assert_eq!(s, v.len());
                                    }
                                    Err(e) => {
                                        println!("udp error: {:?}", e);
                                        return Err(e.into());
                                    }
                                }
                            } else {
                                warn!("BlockToUdp: received wrong PMT type. {:?}", p);
                            }
                            Ok(Pmt::Null)
                        }
                        .boxed()
                    },
                )
                .build(),
            BlobToUdp {
                socket: None,
                remote: remote
                    .as_ref()
                    .to_socket_addrs()
                    .expect("could not resolve socket address")
                    .next()
                    .unwrap(),
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for BlobToUdp {
    async fn init(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let socket = UdpSocket::bind("127.0.0.1:0").await?;
        self.socket = Some(socket);
        Ok(())
    }
}
