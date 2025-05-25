use anyhow::Context;
use async_net::TcpListener;
use async_net::TcpStream;
use futures::AsyncReadExt;

use crate::prelude::*;

/// Read samples from a TCP socket.
#[derive(Block)]
pub struct TcpSource<T, O = DefaultCpuWriter<T>>
where
    T: Send + 'static,
    O: CpuBufferWriter<Item = T>,
{
    #[output]
    output: O,
    bind: String,
    listener: Option<TcpListener>,
    socket: Option<TcpStream>,
}

impl<T, O> TcpSource<T, O>
where
    T: Send + 'static,
    O: CpuBufferWriter<Item = T>,
{
    /// Create TCP Source block
    pub fn new(bind: impl Into<String>) -> Self {
        Self {
            output: O::default(),
            bind: bind.into(),
            listener: None,
            socket: None,
        }
    }
}

#[doc(hidden)]
impl<T, O> Kernel for TcpSource<T, O>
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

        let out = self.output.slice();
        if out.is_empty() {
            return Ok(());
        }
        let out_len = out.len();
        let ptr = out.as_mut_ptr() as *mut u8;
        let byte_len = std::mem::size_of_val(out);
        let data = unsafe { std::slice::from_raw_parts_mut(ptr, byte_len) };

        match self
            .socket
            .as_mut()
            .context("no socket")?
            .read_exact(data)
            .await
        {
            Ok(_) => {
                debug!("tcp source read bytes {}", out.len());
                self.output.produce(out_len);
            }
            Err(_) => {
                debug!("tcp source socket closed");
                io.finished = true;
            }
        }

        Ok(())
    }

    async fn init(&mut self, _mio: &mut MessageOutputs, _meta: &mut BlockMeta) -> Result<()> {
        self.listener = Some(TcpListener::bind(self.bind.clone()).await?);
        Ok(())
    }
}
