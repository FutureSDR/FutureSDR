use std::any::Any;
use std::fmt;

use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::PortId;
use crate::runtime::Result;
use crate::runtime::buffer::BufferReader;
use crate::runtime::dev::BlockInbox;
use crate::runtime::dev::MaybeSend;
use futuresdr::channel::mpsc::Sender;
use futuresdr::runtime::BlockId;

#[async_trait]
/// Block interface, implemented for wrapped kernel instances.
pub trait Block: MaybeSend + Any {
    /// required for downcasting
    fn as_any(&self) -> &dyn Any;
    /// required for downcasting
    fn as_any_mut(&mut self) -> &mut dyn Any;

    // ##### BLOCK
    /// Run the block.
    async fn run(&mut self, main_inbox: Sender<FlowgraphMessage>);
    /// Get the inbox of the block
    fn inbox(&self) -> BlockInbox;
    /// Get the ID of the block
    fn id(&self) -> BlockId;

    // ##### Stream Ports
    /// Get dyn reference to stream input
    fn stream_input(&mut self, id: &PortId) -> Result<&mut dyn BufferReader, Error>;
    /// Connect dyn BufferReader by downcasting it
    fn connect_stream_output(
        &mut self,
        id: &PortId,
        reader: &mut dyn BufferReader,
    ) -> Result<(), Error>;

    // ##### Message Ports
    /// Message inputs of the block
    fn message_inputs(&self) -> &'static [&'static str];
    /// Connect message output port
    fn connect(
        &mut self,
        src_port: &PortId,
        sender: BlockInbox,
        dst_port: &PortId,
    ) -> Result<(), Error>;

    // ##### META
    /// Get instance name (see [`crate::runtime::dev::BlockMeta::instance_name`])
    fn instance_name(&self) -> Option<&str>;
    /// Set instance name (see [`crate::runtime::dev::BlockMeta::set_instance_name`])
    fn set_instance_name(&mut self, name: &str);
    /// Get the static type name of the block.
    fn type_name(&self) -> &str;
    /// Check whether this block is blocking.
    ///
    /// Blocking blocks will be spawned in a separate thread.
    fn is_blocking(&self) -> bool;
}

impl fmt::Debug for dyn Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Block")
            .field("type_name", &self.type_name().to_string())
            .finish()
    }
}
