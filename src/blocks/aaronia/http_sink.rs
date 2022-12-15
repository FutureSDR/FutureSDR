use hyper::{Body, Client, Method, Request};
use num_complex::Complex32;
use serde_json::json;
use std::time::SystemTime;

use crate::anyhow::Result;
use crate::blocks::aaronia::FutureSdrConnector;
use crate::blocks::aaronia::HyperExecutor;
use crate::runtime::scheduler::Scheduler;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::ItemTag;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::Tag;
use crate::runtime::WorkIo;

pub struct HttpSink {
    url: String,
    client: Client<FutureSdrConnector, Body>,
    frequency: f64,
    sample_rate: f64,
}

impl HttpSink {
    pub fn new<S: Scheduler + Send + Sync, I: Into<String>>(
        scheduler: S,
        url: I,
        frequency: f64,
        sample_rate: f64,
    ) -> Block {
        let client = Client::builder()
            .executor(HyperExecutor(scheduler))
            .build::<_, Body>(FutureSdrConnector);

        Block::new(
            BlockMetaBuilder::new("HttpSink").build(),
            StreamIoBuilder::new().add_input::<Complex32>("in").build(),
            MessageIoBuilder::new().build(),
            Self {
                url: url.into(),
                client,
                frequency,
                sample_rate,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for HttpSink {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let input = sio.input(0).slice_unchecked::<f32>();

        let t = sio.input(0).tags().iter().find_map(|x| match x {
            ItemTag {
                index,
                tag: Tag::NamedUsize(n, len),
            } => {
                if *index == 0 && n == "burst_start" {
                    Some(*len)
                } else {
                    None
                }
            }
            _ => None,
        });
        if let Some(len) = t {
            if input.len() >= len * 2 {
                let start = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_secs_f64()
                    + 0.8;
                let stop = start + len as f64 / self.sample_rate;

                let j = json!({
                    "startTime": start,
                    "endTime": stop,
                    "startFrequency": self.frequency - self.sample_rate / 2.0,
                    "endFrequency": self.frequency + self.sample_rate / 2.0,
                    "payload": "iq",
                    "flush": true,
                    "push": true,
                    "format": "json",
                    "samples": input[0..2 * len],
                });

                // println!("{}", j.to_string());

                let req = Request::builder()
                    .method(Method::POST)
                    .uri(format!("{}/sample", self.url))
                    .header("content-type", "application/json")
                    .body(Body::from(j.to_string()))?;

                let _ = self.client.request(req).await?;

                sio.input(0).consume(len);
                if input.len() > len * 2 {
                    io.call_again = true;
                }
            }
        }

        if sio.input(0).finished() {
            io.finished = true;
        }

        Ok(())
    }
}
