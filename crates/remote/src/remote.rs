use futures::Future;
use futuresdr_types::BlockDescription;
use futuresdr_types::FlowgraphDescription;
use futuresdr_types::Pmt;
use hyper::client::connect::Connect;
use hyper::client::HttpConnector;
use hyper::rt::Executor;
use hyper::Body;
use hyper::Client;
use hyper::Request;
use serde::Deserialize;
use std::pin::Pin;

use crate::Error;

async fn get<H: Connect + Clone + Send + Sync + 'static, T: for<'a> Deserialize<'a>>(
    client: &Client<H>,
    url: String,
) -> Result<T, Error> {
    let url: hyper::Uri = url.parse()?;
    let body = match client.get(url.clone()).await {
        Ok(b) => {
            if !b.status().is_success() {
                return Err(Error::Endpoint(url));
            }
            b.into_body()
        }
        Err(e) => return Err(e.into()),
    };
    let bytes = hyper::body::to_bytes(body).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// Connection to a remote runtime.
pub struct Remote<H: Connect + Clone + Send + Sync + 'static> {
    client: Client<H>,
    url: String,
}

impl Remote<HttpConnector> {
    /// Create a [`Remote`].
    pub fn new<I: Into<String>>(url: I) -> Self {
        Self {
            client: Client::new(),
            url: url.into(),
        }
    }
}

impl<H: Connect + Clone + Send + Sync + 'static> Remote<H> {
    /// Create a [`Remote`] with an async runtime.
    pub fn with_runtime<E>(url: String, connector: H, executor: E) -> Self
    where
        E: Executor<Pin<Box<dyn Future<Output = ()> + Send>>> + Send + Sync + 'static,
    {
        let client = Client::builder().executor(executor).build(connector);
        Self { client, url }
    }

    /// Get a specific [`Flowgraph`].
    pub async fn flowgraph(&self, id: usize) -> Result<Flowgraph<H>, Error> {
        let fgs = self.flowgraphs().await?;
        fgs.iter()
            .find(|x| x.id == id)
            .cloned()
            .ok_or(Error::FlowgraphId(id))
    }

    /// Get a list of all running [`Flowgraphs`](Flowgraph).
    pub async fn flowgraphs(&self) -> Result<Vec<Flowgraph<H>>, Error> {
        let ids: Vec<usize> = get(&self.client, format!("{}/api/fg/", self.url)).await?;
        let mut v = Vec::new();

        for i in ids.into_iter() {
            let fg: FlowgraphDescription =
                get(&self.client, format!("{}/api/fg/{}/", self.url, i)).await?;
            v.push(fg);
        }

        let v = v
            .into_iter()
            .enumerate()
            .map(|(i, f)| Flowgraph {
                id: i,
                description: f,
                client: self.client.clone(),
                url: self.url.clone(),
            })
            .collect();

        Ok(v)
    }
}

/// A remote Flowgraph.
#[derive(Clone, Debug)]
pub struct Flowgraph<H: Connect + Clone + Send + Sync + 'static> {
    id: usize,
    description: FlowgraphDescription,
    client: Client<H>,
    url: String,
}

impl<H: Connect + Clone + Send + Sync + 'static> Flowgraph<H> {
    /// Update the [`Flowgraph`], getting current blocks and connections.
    pub async fn update(&mut self) -> Result<(), Error> {
        self.description = get(&self.client, format!("{}/api/fg/{}/", self.url, self.id)).await?;
        Ok(())
    }

    /// Get a list of the [`Blocks`](Block) of the [`Flowgraph`].
    pub fn blocks(&self) -> Vec<Block<H>> {
        self.description
            .blocks
            .iter()
            .map(|d| Block {
                description: d.clone(),
                client: self.client.clone(),
                url: self.url.clone(),
                flowgraph_id: self.id,
            })
            .collect()
    }

    /// Get a specific [`Block`](Block) of the [`Flowgraph`].
    pub fn block(&self, id: usize) -> Option<Block<H>> {
        self.description
            .blocks
            .iter()
            .find(|x| x.id == id)
            .map(|d| Block {
                description: d.clone(),
                client: self.client.clone(),
                url: self.url.clone(),
                flowgraph_id: self.id,
            })
    }

    /// Get a list of all message [`Connections`](Connection) of the [`Flowgraph`].
    pub fn message_connections(&self) -> Vec<Connection<H>> {
        self.description
            .message_edges
            .iter()
            .map(|d| Connection {
                connection_type: ConnectionType::Message,
                src_block: self.block(d.0).unwrap(),
                src_port: d.1,
                dst_block: self.block(d.2).unwrap(),
                dst_port: d.3,
            })
            .collect()
    }

    /// Get a list of all stream [`Connections`](Connection) of the [`Flowgraph`].
    pub fn stream_connections(&self) -> Vec<Connection<H>> {
        self.description
            .stream_edges
            .iter()
            .map(|d| Connection {
                connection_type: ConnectionType::Stream,
                src_block: self.block(d.0).unwrap(),
                src_port: d.1,
                dst_block: self.block(d.2).unwrap(),
                dst_port: d.3,
            })
            .collect()
    }
}

impl<H: Connect + Clone + Send + Sync + 'static> std::fmt::Display for Flowgraph<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Flowgraph {} (B {} / S {} / M {})",
            self.id,
            self.description.blocks.len(),
            self.description.stream_edges.len(),
            self.description.message_edges.len()
        )
    }
}

/// Specify a message handler of a [`Block`]
#[derive(Clone, Debug)]
pub enum Handler {
    /// Nueric ID of the handler
    Id(usize),
    /// Name of the handler
    Name(String),
}

/// A [`Block`] of a [`Flowgraph`].
#[derive(Clone, Debug)]
pub struct Block<H: Connect + Clone + Send + Sync + 'static> {
    description: BlockDescription,
    client: Client<H>,
    url: String,
    flowgraph_id: usize,
}

impl<H: Connect + Clone + Send + Sync + 'static> Block<H> {
    /// Update the [`Block`], retrieving a new [`BlockDescription`] from the [`Flowgraph`].
    pub async fn update(&mut self) -> Result<(), Error> {
        self.description = get(
            &self.client,
            format!(
                "{}/api/fg/{}/block/{}/",
                self.url, self.flowgraph_id, self.description.id
            ),
        )
        .await?;
        Ok(())
    }

    /// Call a message handler of a [`Block`], providing it a [`Pmt::Null`](futuresdr_types::Pmt).
    ///
    /// This is usually used, when the caller is only interested in the return value. The handler
    /// might, for example, just return a parameter (think `get_frequency`, `get_gain`, etc).
    pub async fn call(&self, handler: Handler) -> Result<Pmt, Error> {
        self.callback(handler, Pmt::Null).await
    }

    /// Call a message handler of a [`Block`] with the given [`Pmt`](futuresdr_types::Pmt).
    pub async fn callback(&self, handler: Handler, pmt: Pmt) -> Result<Pmt, Error> {
        let json = serde_json::to_string(&pmt)?;
        let url: hyper::Uri = match handler {
            Handler::Name(n) => format!(
                "{}/api/fg/{}/block/{}/call/{}/",
                &self.url, self.flowgraph_id, self.description.id, n
            ),
            Handler::Id(i) => format!(
                "{}/api/fg/{}/block/{}/call/{}/",
                &self.url, self.flowgraph_id, self.description.id, i
            ),
        }
        .parse()?;
        let req = Request::post(url.clone())
            .header(hyper::header::CONTENT_TYPE, "application/json")
            .body(Body::from(json))?;
        let body = match self.client.request(req).await {
            Ok(b) => {
                if !b.status().is_success() {
                    return Err(Error::Endpoint(url));
                }
                b.into_body()
            }
            Err(e) => return Err(e.into()),
        };
        let bytes = hyper::body::to_bytes(body).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }
}

impl<H: Connect + Clone + Send + Sync + 'static> std::fmt::Display for Block<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({}, {})",
            &self.description.instance_name, &self.description.type_name, self.description.id,
        )
    }
}

/// Connection type for a [`Connection`] between [`Blocks`](Block)
#[derive(Debug, Clone)]
pub enum ConnectionType {
    /// Stream Connection
    Stream,
    /// Message Connection
    Message,
}

/// A Connection between [`Blocks`](Block)
#[derive(Clone, Debug)]
pub struct Connection<H: Connect + Clone + Send + Sync + 'static> {
    connection_type: ConnectionType,
    src_block: Block<H>,
    src_port: usize,
    dst_block: Block<H>,
    dst_port: usize,
}

impl<H: Connect + Clone + Send + Sync + 'static> std::fmt::Display for Connection<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.connection_type {
            ConnectionType::Stream => write!(
                f,
                "{}.{} > {}.{}",
                self.src_block.description.instance_name,
                &self.src_block.description.stream_outputs[self.src_port],
                self.dst_block.description.instance_name,
                &self.dst_block.description.stream_inputs[self.dst_port]
            ),
            ConnectionType::Message => write!(
                f,
                "{}.{} | {}.{}",
                self.src_block.description.instance_name,
                &self.src_block.description.message_outputs[self.src_port],
                self.dst_block.description.instance_name,
                &self.dst_block.description.message_inputs[self.dst_port]
            ),
        }
    }
}
