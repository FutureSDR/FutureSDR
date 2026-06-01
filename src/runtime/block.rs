use std::any::Any;
use std::fmt;

use crate::runtime::BlockId;
use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::PortId;
use crate::runtime::Result;
use crate::runtime::block_inbox::BlockEndpoint;
use crate::runtime::block_inbox::BlockInboxReader;
use crate::runtime::block_inbox::LocalBlockInbox;
use crate::runtime::buffer::AnyBufferReader;
use crate::runtime::buffer::AnyBufferWriterToken;
use crate::runtime::buffer::AnySendBufferWriterToken;
use crate::runtime::channel::mpsc::Sender;

/// Object-safe runtime interface shared by normal and local block wrappers.
pub trait BlockObject: Any {
    /// Return this block as [`Any`] for downcasting.
    fn as_any(&self) -> &dyn Any;
    /// Return this block as mutable [`Any`] for downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Get the send-safe endpoint of the block.
    fn inbox(&self) -> BlockEndpoint;
    /// Get the local inbox handle for local-domain direct delivery.
    fn local_inbox(&self) -> Option<LocalBlockInbox> {
        None
    }
    /// Take the external normal inbox reader for a local-domain block.
    fn take_external_inbox_reader(&mut self) -> Option<BlockInboxReader> {
        None
    }
    /// Get the block id.
    fn id(&self) -> BlockId;

    /// Get a type-erased stream input by port id.
    fn stream_input(&mut self, id: &PortId) -> Result<&mut dyn AnyBufferReader, Error>;
    /// Create an in-domain token for connecting a stream output.
    fn stream_output_token(
        &mut self,
        id: &PortId,
    ) -> Result<Box<dyn AnyBufferWriterToken + '_>, Error>;
    /// Temporarily take a sendable stream output token for cross-domain setup.
    fn take_send_stream_output_token(
        &mut self,
        id: &PortId,
    ) -> Result<Box<dyn AnySendBufferWriterToken>, Error>;
    /// Restore a stream output token that was temporarily taken for cross-domain setup.
    fn replace_send_stream_output_token(
        &mut self,
        id: &PortId,
        token: Box<dyn AnySendBufferWriterToken>,
    ) -> Result<(), Error>;

    /// Message input port names declared by this block.
    fn message_inputs(&self) -> &'static [&'static str];
    /// Message output port names declared by this block.
    fn message_outputs(&self) -> &'static [&'static str] {
        &[]
    }
    /// Connect one message output port to a downstream block endpoint.
    fn connect(
        &mut self,
        src_port: &PortId,
        sender: BlockEndpoint,
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
