use async_net::SocketAddr;
use async_net::UdpSocket;
use std::net::ToSocketAddrs;

use crate::runtime::BlockMeta;
use crate::runtime::Kernel;
use crate::runtime::MessageOutputs;
use crate::runtime::Pmt;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Push [Blobs](crate::runtime::Pmt::Blob) into a UDP socket.
#[derive(Block)]
#[message_inputs(r#in)]
pub struct BlobToUdp {
    socket: Option<UdpSocket>,
    remote: SocketAddr,
}

impl BlobToUdp {
    /// Create [`BlobToUdp`] block
    ///
    /// ## Parameter
    /// - `remote`: UDP socket address, e.g., `localhost:2342`
    pub fn new<S>(remote: S) -> TypedBlock<Self>
    where
        S: AsRef<str>,
    {
        TypedBlock::new(
            StreamIoBuilder::new().build(),
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

    async fn r#in(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        match p {
            Pmt::Blob(v) => match self.socket.as_ref().unwrap().send_to(&v, self.remote).await {
                Ok(s) => {
                    assert_eq!(s, v.len());
                }
                Err(e) => {
                    println!("udp error: {e:?}");
                    return Err(e.into());
                }
            },
            Pmt::Finished => {
                io.finished = true;
            }
            _ => {
                warn!("BlockToUdp: received wrong PMT type. {:?}", p);
            }
        }
        Ok(Pmt::Null)
    }
}

#[doc(hidden)]
impl Kernel for BlobToUdp {
    async fn init(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let socket = UdpSocket::bind("127.0.0.1:0").await?;
        self.socket = Some(socket);
        Ok(())
    }
}
