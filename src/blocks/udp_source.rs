use anyhow::Context;
use async_net::UdpSocket;

use crate::prelude::*;

/// Read samples from a UDP socket.
#[derive(Block)]
pub struct UdpSource<T, O = circular::Writer<T>>
where
    T: Send + 'static,
    O: CpuBufferWriter<Item = T>,
{
    #[output]
    output: O,
    bind: String,
    max_packet_bytes: usize,
    socket: Option<UdpSocket>,
}

impl<T, O> UdpSource<T, O>
where
    T: Send + 'static,
    O: CpuBufferWriter<Item = T>,
{
    /// Create UDP Source block
    pub fn new(bind: impl Into<String>, max_packet_bytes: usize) -> Self {
        Self {
            output: O::default(),
            bind: bind.into(),
            max_packet_bytes,
            socket: None,
        }
    }
}

#[doc(hidden)]
impl<T, O> Kernel for UdpSource<T, O>
where
    T: Send + 'static,
    O: CpuBufferWriter<Item = T>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let out = self.output.slice();
        let ptr = out.as_mut_ptr() as *mut u8;
        let byte_len = std::mem::size_of_val(out);
        let data = unsafe { std::slice::from_raw_parts_mut(ptr, byte_len) };

        if byte_len < self.max_packet_bytes {
            return Ok(());
        }

        match self
            .socket
            .as_ref()
            .context("no socket")?
            .recv_from(data)
            .await
        {
            Ok((s, _)) => {
                debug!("udp source read bytes {}", s);
                self.output.produce(s / std::mem::size_of::<T>());
            }
            Err(_) => {
                debug!("udp source socket closed");
                io.finished = true;
            }
        }

        Ok(())
    }

    async fn init(&mut self, _mio: &mut MessageOutputs, _meta: &mut BlockMeta) -> Result<()> {
        self.socket = Some(UdpSocket::bind(self.bind.clone()).await?);
        Ok(())
    }
}
