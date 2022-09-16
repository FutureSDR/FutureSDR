//! ## SDR Runtime
use futures::channel::mpsc;
use futures::channel::oneshot;

mod block;
mod block_meta;
pub mod buffer;
pub mod config;

#[cfg(not(target_arch = "wasm32"))]
pub mod ctrl_port;
#[cfg(target_arch = "wasm32")]
pub mod ctrl_port {
    pub use futuresdr_pmt::BlockDescription;
    pub use futuresdr_pmt::FlowgraphDescription;
}
use crate::runtime::ctrl_port::BlockDescription;
use crate::runtime::ctrl_port::FlowgraphDescription;

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
mod logging;
#[cfg(target_os = "android")]
#[path = "logging_android.rs"]
mod logging;
#[cfg(target_arch = "wasm32")]
#[path = "logging_wasm.rs"]
mod logging;

mod flowgraph;
pub mod message_io;
mod mocker;
#[allow(clippy::module_inception)]
mod runtime;
pub mod scheduler;
pub mod stream_io;
mod tag;
mod topology;

pub use block::Block;
pub use block::Kernel;
pub use block::WorkIo;
pub use block_meta::BlockMeta;
pub use block_meta::BlockMetaBuilder;
pub use flowgraph::Flowgraph;
pub use flowgraph::FlowgraphHandle;
pub use futuresdr_pmt::Pmt;
pub use message_io::MessageInput;
pub use message_io::MessageIo;
pub use message_io::MessageIoBuilder;
pub use message_io::MessageOutput;
pub use mocker::Mocker;
pub(crate) use runtime::run_block;
pub use runtime::Runtime;
pub use runtime::RuntimeBuilder;
pub use stream_io::StreamInput;
pub use stream_io::StreamIo;
pub use stream_io::StreamIoBuilder;
pub use stream_io::StreamOutput;
pub use tag::ItemTag;
pub use tag::Tag;
pub use topology::Topology;

use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;

pub fn init() {
    logging::init();
}

#[derive(Debug)]
pub enum FlowgraphMessage {
    Terminate,
    Initialized,
    BlockDone {
        block_id: usize,
        block: Block,
    },
    BlockCall {
        block_id: usize,
        port_id: usize,
        data: Pmt,
    },
    BlockCallback {
        block_id: usize,
        port_id: usize,
        data: Pmt,
        tx: oneshot::Sender<Pmt>,
    },
    FlowgraphDescription {
        tx: oneshot::Sender<FlowgraphDescription>,
    },
    BlockDescription {
        block_id: usize,
        tx: oneshot::Sender<BlockDescription>,
    },
}

#[derive(Debug)]
pub enum BlockMessage {
    Initialize,
    Terminate,
    Notify,
    BlockDescription {
        tx: oneshot::Sender<BlockDescription>,
    },
    StreamOutputInit {
        src_port: usize,
        writer: BufferWriter,
    },
    StreamInputInit {
        dst_port: usize,
        reader: BufferReader,
    },
    StreamInputDone {
        input_id: usize,
    },
    StreamOutputDone {
        output_id: usize,
    },
    MessageOutputConnect {
        src_port: usize,
        dst_port: usize,
        dst_inbox: mpsc::Sender<BlockMessage>,
    },
    Call {
        port_id: usize,
        data: Pmt,
    },
    Callback {
        port_id: usize,
        data: Pmt,
        tx: oneshot::Sender<Pmt>,
    },
}
