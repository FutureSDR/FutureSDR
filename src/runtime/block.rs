use std::any::Any;
use std::fmt;

use crate::runtime::BlockId;
use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::PortId;
use crate::runtime::PortIndex;
use crate::runtime::Result;
use crate::runtime::block_inbox::BlockEndpoint;
use crate::runtime::block_inbox::BlockInboxReader;
use crate::runtime::block_inbox::LocalBlockInbox;
use crate::runtime::buffer::DynBufferReader;
use crate::runtime::buffer::DynBufferWriter;
use crate::runtime::channel::mpsc::Sender;

/// Internal object-safe erased interface shared by normal and local block wrappers.
///
/// Custom block authors implement [`Kernel`](crate::runtime::dev::Kernel);
/// scheduler extensions use opaque runnable/stopped block handles. This trait
/// is runtime plumbing for storing wrapped kernels after type erasure and
/// installing dynamic edges.
pub(crate) trait BlockObject: Any {
    /// Get the send-safe endpoint of the block.
    fn inbox(&self) -> BlockEndpoint;
    /// Get the block id.
    fn id(&self) -> BlockId;
    /// Get the static type name of the block.
    fn type_name(&self) -> &str;
    /// Get the current runtime instance name of the block.
    fn instance_name(&self) -> Option<&str>;

    /// Get stream input port names declared by this block.
    fn stream_input_names(&mut self) -> Result<Vec<String>, Error>;
    /// Get stream output port names declared by this block.
    fn stream_output_names(&mut self) -> Result<Vec<String>, Error>;
    /// Get a type-erased stream input by port id.
    fn stream_input(&mut self, id: &PortId) -> Result<&mut dyn DynBufferReader, Error>;
    /// Get a type-erased stream output by port id.
    fn stream_output(&mut self, id: &PortId) -> Result<&mut dyn DynBufferWriter, Error>;
    /// Message input port names declared by this block.
    fn message_inputs(&self) -> &'static [&'static str];
    /// Message output port names declared by this block.
    fn message_outputs(&self) -> &'static [&'static str];
    /// Connect one message output port to a downstream block endpoint.
    fn connect_message(
        &mut self,
        src_port: PortIndex,
        dst: BlockEndpoint,
        dst_port: PortIndex,
    ) -> Result<(), Error>;
}

/// Internal object-safe interface for normal-domain wrapped kernel instances.
///
/// Custom blocks implement [`Kernel`](crate::runtime::dev::Kernel). Scheduler
/// extensions receive [`RunnableBlock`](crate::runtime::scheduler::RunnableBlock)
/// instead of raw block trait objects.
#[async_trait::async_trait]
pub(crate) trait Block: BlockObject + Send {
    /// Run the block.
    async fn run(&mut self, main_inbox: Sender<FlowgraphMessage>);
}

impl fmt::Debug for dyn Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Block")
            .field("type_name", &self.type_name().to_string())
            .finish()
    }
}

/// Runtime interface for blocks that stay inside a local domain.
///
/// This is separate from [`Block`] because the future returned by `run()` is not
/// required to be `Send`; local-domain blocks never move between worker threads.
#[async_trait::async_trait(?Send)]
pub(crate) trait LocalBlock: BlockObject {
    /// Get the local inbox handle for local-domain direct delivery.
    fn local_inbox(&self) -> LocalBlockInbox;
    /// Take the external normal inbox reader for forwarding into the local domain.
    fn take_external_inbox_reader(&mut self) -> Option<BlockInboxReader>;

    async fn run(&mut self, main_inbox: Sender<FlowgraphMessage>);
}

impl fmt::Debug for dyn LocalBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalBlock")
            .field("type_name", &self.type_name().to_string())
            .finish()
    }
}
