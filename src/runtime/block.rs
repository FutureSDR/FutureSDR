use std::any::Any;
use std::fmt;

use crate::runtime::BlockId;
use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::PortId;
use crate::runtime::Result;
use crate::runtime::block_inbox::BlockInbox;
use crate::runtime::buffer::BufferReader;
use crate::runtime::channel::mpsc::Sender;

/// Object-safe runtime interface shared by normal and local block wrappers.
pub trait BlockObject: Any {
    /// Return this block as [`Any`] for downcasting.
    fn as_any(&self) -> &dyn Any;
    /// Return this block as mutable [`Any`] for downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Get the sender-side inbox of the block.
    fn inbox(&self) -> BlockInbox;
    /// Get the block id.
    fn id(&self) -> BlockId;

    /// Get a type-erased stream input by port id.
    fn stream_input(&mut self, id: &PortId) -> Result<&mut dyn BufferReader, Error>;
    /// Connect a type-erased stream output by downcasting the destination reader.
    fn connect_stream_output(
        &mut self,
        id: &PortId,
        reader: &mut dyn BufferReader,
    ) -> Result<(), Error>;

    /// Message input port names declared by this block.
    fn message_inputs(&self) -> &'static [&'static str];
    /// Connect one message output port to a downstream block inbox.
    fn connect(
        &mut self,
        src_port: &PortId,
        sender: BlockInbox,
        dst_port: &PortId,
    ) -> Result<(), Error>;

    /// Get the static type name of the block.
    fn type_name(&self) -> &str;
    /// Whether this block is flagged for a local blocking domain.
    fn is_blocking(&self) -> bool;
}

/// Runtime object-safe interface for wrapped kernel instances.
///
/// Custom blocks implement [`Kernel`](crate::runtime::dev::Kernel); this trait
/// is implemented by the normal runtime wrapper around send-capable kernels and
/// is mainly useful for runtime extensions.
#[async_trait::async_trait]
pub trait Block: BlockObject + Send {
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
    async fn run(&mut self, main_inbox: Sender<FlowgraphMessage>);
}

impl fmt::Debug for dyn LocalBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalBlock")
            .field("type_name", &self.type_name().to_string())
            .finish()
    }
}
