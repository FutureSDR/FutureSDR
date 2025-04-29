use std::future::Future;

use futuresdr::channel::mpsc::Sender;
use futuresdr::runtime::buffer::BufferReader;
use futuresdr::runtime::BlockId;
use futuresdr::runtime::BlockMessage;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::Error;
use futuresdr::runtime::MessageOutputs;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::PortId;
use futuresdr::runtime::Result;
use futuresdr::runtime::WorkIo;

/// Kernal
///
/// Central trait to implement a block
#[cfg(not(target_arch = "wasm32"))]
pub trait Kernel: Send {
    /// Processes stream data
    fn work(
        &mut self,
        _io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> impl Future<Output = Result<()>> + Send {
        async { Ok(()) }
    }
    /// Initialize kernel
    fn init(
        &mut self,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> impl Future<Output = Result<()>> + Send {
        async { Ok(()) }
    }
    /// De-initialize kernel
    fn deinit(
        &mut self,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> impl Future<Output = Result<()>> + Send {
        async { Ok(()) }
    }
}

/// Kernal
///
/// Central trait to implement a block
#[cfg(target_arch = "wasm32")]
pub trait Kernel: Send {
    /// Processes stream data
    fn work(
        &mut self,
        _io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> impl Future<Output = Result<()>> {
        async { Ok(()) }
    }
    /// Initialize kernel
    fn init(
        &mut self,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> impl Future<Output = Result<()>> {
        async { Ok(()) }
    }
    /// De-initialize kernel
    fn deinit(
        &mut self,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> impl Future<Output = Result<()>> {
        async { Ok(()) }
    }
}

/// Interface to the Kernel, implemented by the block macro.
#[cfg(not(target_arch = "wasm32"))]
pub trait KernelInterface {
    /// If true, the block is run in a spearate thread
    fn is_blocking() -> bool;
    /// Name of the block
    fn type_name() -> &'static str;
    /// Input Stream Ports
    fn stream_inputs(&self) -> Vec<String>;
    /// Output Stream Ports.
    fn stream_outputs(&self) -> Vec<String>;
    /// Initialize Stream Ports
    ///
    /// This sets required variables but does not connect.
    fn stream_ports_init(&mut self, block_id: BlockId, inbox: Sender<BlockMessage>);
    /// This sets required variables but does not connect.
    fn stream_ports_validate(&self) -> Result<(), Error>;
    /// Mark stream input as finished
    fn stream_input_finish(&mut self, port_id: PortId) -> Result<(), Error>;
    /// Tell adjacent blocks that we are done
    fn stream_ports_notify_finished(&mut self) -> impl Future<Output = ()> + Send;
    /// Get dyn reference to stream input
    fn stream_input(&mut self, name: &str) -> Option<&mut dyn BufferReader>;
    /// Connect dyn BufferReader by downcasting it
    fn connect_stream_output(
        &mut self,
        name: &str,
        reader: &mut dyn BufferReader,
    ) -> Result<(), Error>;

    /// Input Message Handler Names.
    fn message_inputs() -> &'static [&'static str];
    /// Output Message Handler Names.
    fn message_outputs() -> &'static [&'static str];
    /// Call message handlers of the kernel.
    fn call_handler(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        id: PortId,
        _p: Pmt,
    ) -> impl Future<Output = Result<Pmt, Error>> + Send;
}

/// Interface to the Kernel, implemented by the block macro.
#[cfg(target_arch = "wasm32")]
pub trait KernelInterface {
    /// Call message handlers of the kernel.
    fn call_handler(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        id: PortId,
        _p: Pmt,
    ) -> impl Future<Output = Result<Pmt, Error>>;
    /// Input Message Handler Names.
    fn message_input_names() -> &'static [&'static str];
    /// Output Message Handler Names.
    fn message_output_names() -> &'static [&'static str];
    /// If true, the block is run in a spearate thread
    fn is_blocking() -> bool;
    /// Name of the block
    fn type_name() -> &'static str;
}
