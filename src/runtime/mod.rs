use futures::channel::mpsc;
use futures::channel::oneshot;

mod block;
mod block_builder;
mod block_meta;
pub mod buffer;
pub mod config;

#[cfg(not(target_arch = "wasm32"))]
pub mod ctrl_port;

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
mod logging;
#[cfg(target_os = "android")]
#[path = "logging_android.rs"]
mod logging;
#[cfg(target_arch = "wasm32")]
#[path = "logging_wasm.rs"]
mod logging;

mod flowgraph;
mod message_io;
#[allow(clippy::module_inception)]
mod runtime;
pub mod scheduler;
mod stream_io;
mod topology;

pub use block::AsyncBlock;
pub use block::AsyncKernel;
pub use block::Block;
pub use block_builder::BlockBuilder;
pub use block::SyncBlock;
pub use block::SyncKernel;
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
pub(crate) use runtime::run_block;
pub use runtime::Runtime;
pub use runtime::RuntimeBuilder;
pub use stream_io::StreamInput;
pub use stream_io::StreamIo;
pub use stream_io::StreamIoBuilder;
pub use stream_io::StreamOutput;
pub use topology::Topology;

use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;

pub fn init() {
    logging::init();
}

#[derive(Debug)]
pub enum AsyncMessage {
    Initialize,
    Initialized,
    Notify,
    Terminate,
    BlockDone {
        id: usize,
        block: Block,
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
        dst_inbox: mpsc::Sender<AsyncMessage>,
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
}
