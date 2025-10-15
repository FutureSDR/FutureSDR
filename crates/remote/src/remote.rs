use futuresdr_types::BlockDescription;
use futuresdr_types::BlockId;
use futuresdr_types::FlowgraphDescription;
use futuresdr_types::Pmt;
use futuresdr_types::PortId;
use reqwest::Client;
use reqwest::IntoUrl;
use serde::Deserialize;

use crate::Error;

async fn get<T: for<'a> Deserialize<'a>>(client: Client, url: impl IntoUrl) -> Result<T, Error> {
    Ok(client.get(url).send().await?.json::<T>().await?)
}

/// Connection to a remote runtime.
pub struct Remote {
    client: Client,
    url: String,
}

impl Remote {
    /// Create a [`Remote`].
    pub fn new<I: Into<String>>(url: I) -> Self {
        Self {
            client: Client::new(),
            url: url.into(),
        }
    }

    /// Get a specific [`Flowgraph`].
    pub async fn flowgraph(&self, id: usize) -> Result<Flowgraph, Error> {
        let fgs = self.flowgraphs().await?;
        fgs.iter()
            .find(|x| x.id == id)
            .cloned()
            .ok_or(Error::FlowgraphId(id))
    }

    /// Get a list of all running [`Flowgraphs`](Flowgraph).
    pub async fn flowgraphs(&self) -> Result<Vec<Flowgraph>, Error> {
        let ids: Vec<usize> = get(self.client.clone(), format!("{}/api/fg/", self.url)).await?;
        let mut v = Vec::new();

        for i in ids.into_iter() {
            let fg: FlowgraphDescription =
                get(self.client.clone(), format!("{}/api/fg/{}/", self.url, i)).await?;
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
pub struct Flowgraph {
    id: usize,
    description: FlowgraphDescription,
    client: Client,
    url: String,
}

impl Flowgraph {
    /// Update the [`Flowgraph`], getting current blocks and connections.
    pub async fn update(&mut self) -> Result<(), Error> {
        self.description = get(
            self.client.clone(),
            format!("{}/api/fg/{}/", self.url, self.id),
        )
        .await?;
        Ok(())
    }

    /// Get a list of the [`Blocks`](Block) of the [`Flowgraph`].
    pub fn blocks(&self) -> Vec<Block> {
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

    /// Get a specific [`Block`](Block) of the [`Flowgraph`] by `id`.
    ///
    /// Returns `None` if `Block` is not found.
    pub fn block(&self, id: BlockId) -> Option<Block> {
        self.block_by(|d| d.id == id)
    }

    /// Get a specific [`Block`](Block) of the [`Flowgraph`] by `instance_name`.
    ///
    /// Returns `None` if `Block` is not found.
    pub fn block_by_name(&self, name: &str) -> Option<Block> {
        self.block_by(|d| d.instance_name == name)
    }

    /// Find the first [`Block`](Block) of the [`Flowgraph`] matching the given predicate
    /// on [`BlockDescription`].
    ///
    /// Returns `None` if no `BlockDescription` matches given predicate.
    pub fn block_by(&self, pred: impl Fn(&BlockDescription) -> bool) -> Option<Block> {
        self.description
            .blocks
            .iter()
            .find(|d| pred(d))
            .map(|d| Block {
                description: d.clone(),
                client: self.client.clone(),
                url: self.url.clone(),
                flowgraph_id: self.id,
            })
    }

    /// Get a list of all message [`Connections`](Connection) of the [`Flowgraph`].
    pub fn message_connections(&self) -> Vec<Connection> {
        self.description
            .message_edges
            .iter()
            .map(|d| Connection {
                connection_type: ConnectionType::Message,
                src_block: self.block(d.0).unwrap(),
                src_port: d.1.clone(),
                dst_block: self.block(d.2).unwrap(),
                dst_port: d.3.clone(),
            })
            .collect()
    }

    /// Get a list of all stream [`Connections`](Connection) of the [`Flowgraph`].
    pub fn stream_connections(&self) -> Vec<Connection> {
        self.description
            .stream_edges
            .iter()
            .map(|d| Connection {
                connection_type: ConnectionType::Stream,
                src_block: self.block(d.0).unwrap(),
                src_port: d.1.clone(),
                dst_block: self.block(d.2).unwrap(),
                dst_port: d.3.clone(),
            })
            .collect()
    }
}

impl std::fmt::Display for Flowgraph {
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
pub struct Block {
    description: BlockDescription,
    client: Client,
    url: String,
    flowgraph_id: usize,
}

impl Block {
    /// Update the [`Block`], retrieving a new [`BlockDescription`] from the [`Flowgraph`].
    pub async fn update(&mut self) -> Result<(), Error> {
        self.description = get(
            self.client.clone(),
            format!(
                "{}/api/fg/{}/block/{}/",
                self.url, self.flowgraph_id, self.description.id.0
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
        let url = match handler {
            Handler::Name(n) => format!(
                "{}/api/fg/{}/block/{}/call/{}/",
                &self.url, self.flowgraph_id, self.description.id.0, n
            ),
            Handler::Id(i) => format!(
                "{}/api/fg/{}/block/{}/call/{}/",
                &self.url, self.flowgraph_id, self.description.id.0, i
            ),
        };

        Ok(self
            .client
            .post(url)
            .json(&pmt)
            .send()
            .await?
            .json::<Pmt>()
            .await?)
    }

    /// BlockDescription
    pub fn description(&self) -> &BlockDescription {
        &self.description
    }
}

impl std::fmt::Display for Block {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({}, {})",
            &self.description.instance_name, &self.description.type_name, self.description.id.0,
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
pub struct Connection {
    connection_type: ConnectionType,
    src_block: Block,
    src_port: PortId,
    dst_block: Block,
    dst_port: PortId,
}

impl Connection {
    /// Connection type
    pub fn connection_type(&self) -> ConnectionType {
        self.connection_type.clone()
    }
    /// Source block
    pub fn src_block(&self) -> &Block {
        &self.src_block
    }
    /// Source port
    pub fn src_port(&self) -> &PortId {
        &self.src_port
    }
    /// Source block
    pub fn dst_block(&self) -> &Block {
        &self.dst_block
    }
    /// Source port
    pub fn dst_port(&self) -> &PortId {
        &self.dst_port
    }
}

impl std::fmt::Display for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.connection_type {
            ConnectionType::Stream => write!(
                f,
                "{}.{} > {}.{}",
                self.src_block.description.instance_name,
                self.src_port.name(),
                self.dst_block.description.instance_name,
                self.dst_port.name()
            ),
            ConnectionType::Message => write!(
                f,
                "{}.{} | {}.{}",
                self.src_block.description.instance_name,
                self.src_port.name(),
                self.dst_block.description.instance_name,
                self.dst_port.name()
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Flowgraph;
    use futuresdr_types::BlockDescription;
    use futuresdr_types::BlockId;
    use futuresdr_types::FlowgraphDescription;
    use futuresdr_types::PortId;

    fn block(id: usize, name: &str) -> BlockDescription {
        BlockDescription {
            id: BlockId(id),
            type_name: "test_block".to_string(),
            instance_name: name.to_string(),
            stream_inputs: vec!["in".to_string()],
            stream_outputs: vec!["out".to_string()],
            message_inputs: vec!["command".to_string()],
            message_outputs: vec!["message".to_string()],
            blocking: false,
        }
    }

    #[test]
    fn find_block() {
        let fg = Flowgraph {
            id: 0,
            description: FlowgraphDescription {
                blocks: vec![block(0, "a"), block(1, "b")],
                stream_edges: vec![(
                    BlockId(0),
                    PortId::new("output"),
                    BlockId(1),
                    PortId::new("input"),
                )],
                message_edges: vec![(
                    BlockId(1),
                    PortId::new("out"),
                    BlockId(0),
                    PortId::new("in"),
                )],
            },
            client: reqwest::Client::new(),
            url: "http://localhost".to_string(),
        };

        assert_eq!(
            fg.block(BlockId(0)).map(|b| b.description.instance_name),
            Some("a".to_string())
        );
        assert_eq!(
            fg.block_by_name("b").map(|b| b.description.id),
            Some(BlockId(1))
        );
        assert!(fg.block_by(|d| d.type_name == "test_block").is_some());
        assert!(fg.block_by(|d| d.type_name == "foo").is_none());
    }
}
