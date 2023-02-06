use async_net::SocketAddr;
use async_net::UdpSocket;
use std::net::ToSocketAddrs;

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
                .add_input("in", Self::handler)
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

    #[message_handler]
    async fn handler(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        if let Pmt::Blob(v) = p {
            match self.socket.as_ref().unwrap().send_to(&v, self.remote).await {
                Ok(s) => {
                    assert_eq!(s, v.len());
                }
                Err(e) => {
                    println!("udp error: {e:?}");
                    return Err(e.into());
                }
            }
        } else {
            warn!("BlockToUdp: received wrong PMT type. {:?}", p);
        }
        Ok(Pmt::Null)
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
