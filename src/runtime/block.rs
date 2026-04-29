use std::any::Any;
use std::fmt;

use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::PortId;
use crate::runtime::Result;
use crate::runtime::buffer::BufferReader;
use crate::runtime::dev::BlockInbox;
use crate::runtime::dev::MaybeSend;
use futuresdr::runtime::BlockId;
use futuresdr::runtime::channel::mpsc::Sender;

#[async_trait]
/// Runtime object-safe interface for wrapped kernel instances.
///
/// Custom blocks implement [`Kernel`](crate::runtime::dev::Kernel); this trait
/// is implemented by the runtime wrapper generated around a kernel and is
/// mainly useful for runtime extensions.
pub trait Block: MaybeSend + Any {
    /// Return this block as [`Any`] for downcasting.
    fn as_any(&self) -> &dyn Any;
    /// Return this block as mutable [`Any`] for downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    // ##### BLOCK
    /// Run the block.
    async fn run(&mut self, main_inbox: Sender<FlowgraphMessage>);
    /// Get the sender-side inbox of the block.
    fn inbox(&self) -> BlockInbox;
    /// Get the block id.
    fn id(&self) -> BlockId;

    // ##### Stream Ports
    /// Get a type-erased stream input by port id.
    fn stream_input(&mut self, id: &PortId) -> Result<&mut dyn BufferReader, Error>;
    /// Connect a type-erased stream output by downcasting the destination reader.
    fn connect_stream_output(
        &mut self,
        id: &PortId,
        reader: &mut dyn BufferReader,
    ) -> Result<(), Error>;

    // ##### Message Ports
    /// Message input port names declared by this block.
    fn message_inputs(&self) -> &'static [&'static str];
    /// Connect one message output port to a downstream block inbox.
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
