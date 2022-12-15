use futures::StreamExt;
use futures::TryStreamExt;
use hyper::body::Buf;
use hyper::body::Bytes;
use hyper::{Body, Client, Request};
use num_complex::Complex32;
use serde_json::Value;

use crate::anyhow::{Context, Result};
use crate::blocks::aaronia::FutureSdrConnector;
use crate::blocks::aaronia::HyperExecutor;
use crate::runtime::scheduler::Scheduler;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

pub struct HttpSource<S: Scheduler> {
    executor: HyperExecutor<S>,
    url: String,
    stream: Option<futures::stream::IntoStream<Body>>,
    buf: Bytes,
    items_left: usize,
}

impl<S: Scheduler + Send + Sync> HttpSource<S> {
    pub fn new<I: Into<String>>(scheduler: S, url: I) -> Block {
        Block::new(
            BlockMetaBuilder::new("HttpSource").build(),
            StreamIoBuilder::new()
                .add_output::<Complex32>("out")
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                executor: HyperExecutor(scheduler),
                url: url.into(),
                stream: None,
                buf: Bytes::new(),
                items_left: 0,
            },
        )
    }

    fn parse_header(&mut self) -> Result<()> {
        if let Some(i) = self.buf.iter().position(|&b| b == 10) {
            let header: Value = serde_json::from_str(&String::from_utf8_lossy(&self.buf[0..i]))?;
            debug!("chunck header {header:?}");
            if self.buf.len() > i + 2 {
                self.buf.advance(i + 2);
            } else {
                self.buf = Bytes::new();
            }
            let i = header
                .get("samples")
                .and_then(|x| x.to_string().parse::<usize>().ok())
                .context("failed to read number of samples")?;
            self.items_left = i;
        }
        Ok(())
    }

    async fn get_data(&mut self) -> Result<()> {
        let b = self
            .stream
            .as_mut()
            .unwrap()
            .next()
            .await
            .context("stream finished")??;
        self.buf = [std::mem::take(&mut self.buf), b].concat().into();
        Ok(())
    }
}

#[doc(hidden)]
#[async_trait]
impl<S: Scheduler + Send + Sync> Kernel for HttpSource<S> {
    async fn init(
        &mut self,
        _s: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let req = Request::get(format!("{}/stream?format=float32", self.url)).body(Body::empty())?;

        let stream = Client::builder()
            .executor(self.executor.clone())
            .build::<_, Body>(FutureSdrConnector)
            .request(req)
            .await?
            .into_body()
            .into_stream();

        self.stream = Some(stream);

        Ok(())
    }

    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let out = sio.output(0).slice_unchecked::<u8>();

        if self.items_left == 0 {
            self.parse_header()?;
            while self.items_left == 0 {
                self.get_data().await?;
                self.parse_header()?
            }
        }

        let is = std::mem::size_of::<Complex32>();
        let n = std::cmp::min(self.buf.len() / is, out.len() / is);
        let n = std::cmp::min(n, self.items_left);

        out[0..n * is].copy_from_slice(&self.buf[0..n * is]);

        if n == self.buf.len() / is {
            self.buf.advance(n * is);
            self.get_data().await?;
        } else {
            self.buf.advance(n * is);
        }

        self.items_left -= n;
        sio.output(0).produce(n);

        if self.items_left == 0 {
            io.call_again = true;
        }

        Ok(())
    }
}
