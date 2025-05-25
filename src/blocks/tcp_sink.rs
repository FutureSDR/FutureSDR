use anyhow::bail;
use anyhow::Context;
use async_net::TcpListener;
use async_net::TcpStream;
use futures::AsyncWriteExt;

use crate::prelude::*;

/// Push samples into a TCP socket.
#[derive(Block)]
pub struct TcpSink<T, I = DefaultCpuReader<T>>
where
    T: Send + 'static,
    I: CpuBufferReader<Item = T>,
{
    #[input]
    input: I,
    port: u32,
    listener: Option<TcpListener>,
    socket: Option<TcpStream>,
}

impl<T, I> TcpSink<T, I>
where
    T: Send + 'static,
    I: CpuBufferReader<Item = T>,
{
    /// Create TCP Sink block
    pub fn new(port: u32) -> Self {
        Self {
            input: I::default(),
            port,
            listener: None,
            socket: None,
        }
    }
}

#[doc(hidden)]
impl<T, I> Kernel for TcpSink<T, I>
where
    T: Send + 'static,
    I: CpuBufferReader<Item = T>,
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
            debug!("tcp sink accepted connection");
        }

        let i = self.input.slice();
        let i_len = i.len();
        let ptr = i.as_ptr() as *const u8;
        let byte_len = std::mem::size_of_val(i);
        let data = unsafe { std::slice::from_raw_parts(ptr, byte_len) };

        match self
            .socket
            .as_mut()
            .context("no socket")?
            .write_all(data)
            .await
        {
            Ok(()) => {}
            Err(_) => bail!("tcp sink socket error"),
        }

        if self.input.finished() {
            io.finished = true;
        }

        self.input.consume(i_len);

        Ok(())
    }

    async fn init(&mut self, _mio: &mut MessageOutputs, _meta: &mut BlockMeta) -> Result<()> {
        self.listener = Some(TcpListener::bind(format!("127.0.0.1:{}", self.port)).await?);
        Ok(())
    }
}
