mod remote;
pub use remote::Remote;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Hyper")]
    Hyper(#[from] hyper::Error),
    #[error("Invalid uri")]
    Uri(#[from] http::uri::InvalidUri) ,
    #[error("IO error")]
    Io(#[from] std::io::Error),
    #[error("Serde error")]
    Serde(#[from] serde_json::Error),
    #[error("Wrong endpoint")]
    Endpoint(hyper::Uri),
    #[error("Wrong flowgraph id")]
    FlowgraphId(usize),
}
