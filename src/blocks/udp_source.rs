use anyhow::Context;
use async_net::UdpSocket;

use crate::runtime::dev::prelude::*;

/// Read samples from a UDP socket.
///
/// # Stream Inputs
///
/// No stream inputs.
///
/// # Stream Outputs
///
/// `output`: Samples decoded from UDP packet bytes.
///
/// # Usage
/// ```no_run
/// use futuresdr::blocks::UdpSource;
///
/// let source = UdpSource::<u8>::new("127.0.0.1:9000", 1500);
/// ```
#[derive(Block)]
pub struct UdpSource<T, O = DefaultCpuWriter<T>>
where
    T: CpuSample + bytemuck::Pod,
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
    T: CpuSample + bytemuck::Pod,
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
    T: CpuSample + bytemuck::Pod,
    O: CpuBufferWriter<Item = T>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
    ) -> Result<()> {
        let out = self.output.slice();
        let data = bytemuck::cast_slice_mut(out);
        let byte_len = data.len();

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
                self.output.produce(s / T::SIZE.get());
            }
            Err(_) => {
                debug!("udp source socket closed");
                io.finished = true;
            }
        }

        Ok(())
    }

    async fn init(&mut self, _mo: &mut MessageOutputs, _meta: &BlockMeta) -> Result<()> {
        self.socket = Some(UdpSocket::bind(self.bind.clone()).await?);
        Ok(())
    }
}
