use futures::channel::mpsc::Receiver;
use futures::channel::mpsc::Sender;
use futures::future::join_all;
use futures::future::Either;
use futures::FutureExt;
use futures::SinkExt;
use futures::StreamExt;
use std::any::Any;
use std::fmt;

use crate::runtime::BlockDescription;
use crate::runtime::BlockMessage;
use crate::runtime::BlockMeta;
use crate::runtime::BlockPortCtx;
use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::Kernel;
use crate::runtime::KernelInterface;
use crate::runtime::MessageOutput;
use crate::runtime::MessageOutputs;
use crate::runtime::Result;
use crate::runtime::StreamInputs;
use crate::runtime::StreamOutputs;
use crate::runtime::WorkIo;

#[async_trait]
/// Block interface, implemented for [TypedBlock]s
pub trait BlockT: Send + Any {
    // ##### BLOCK
    /// Cast block to [std::any::Any].
    fn as_any(&self) -> &dyn Any;
    /// Cast block to [std::any::Any] mutably.
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// Run the block.
    async fn run(
        &mut self,
        block_id: usize,
        main_inbox: Sender<FlowgraphMessage>,
        inbox: Receiver<BlockMessage>,
    ) -> Result<(), Error>;

    // ##### META
    /// Get instance name (see [`BlockMeta::instance_name`])
    fn instance_name(&self) -> Option<&str>;
    /// Set instance name (see [`BlockMeta::set_instance_name`])
    fn set_instance_name(&mut self, name: &str);
    /// Get type name (see [`BlockMeta::type_name`])
    fn type_name(&self) -> &str;
    /// Check whether this block is blocking.
    ///
    /// Blocking blocks will be spawned in a separate thread.
    fn is_blocking(&self) -> bool;

    // ##### STREAM IO
    /// Map stream input port name ot id
    fn stream_input_names(&self) -> Vec<String>;
    /// Map stream input port name ot id
    fn stream_output_names(&self) -> Vec<String>;

    // ##### MESSAGE IO
    /// Map message input port name to id
    fn message_input_name_to_id(&self, name: &str) -> Option<usize>;
    /// Get message output ports
    fn message_outputs(&self) -> &Vec<MessageOutput>;
    /// Map message output port name to id
    fn message_output_name_to_id(&self, name: &str) -> Option<usize>;
}

/// Typed Block
pub struct TypedBlock<K: Kernel<I, O>, I: StreamInputs, O: StreamOutputs> {
    /// Block metadata
    pub meta: BlockMeta,
    /// Stream Inputs
    pub inputs: I,
    /// Stream Outputs
    pub outputs: O,
    /// Message IO
    pub mio: MessageOutputs,
    /// Kernel
    pub kernel: K,
}

impl<K: KernelInterface + Kernel<I, O> + Send + 'static, I: StreamInputs, O: StreamOutputs>
    TypedBlock<K, I, O>
{
    /// Create Typed Block
    pub fn new(kernel: K, inputs: I, outputs: O) -> Self {
        Self {
            meta: BlockMeta::new(),
            inputs,
            outputs,
            mio: MessageOutputs::new(
                K::message_output_names()
                    .iter()
                    .map(|x| x.to_string())
                    .collect(),
            ),
            kernel,
        }
    }

    async fn run_impl(
        &mut self,
        block_id: usize,
        mut main_inbox: Sender<FlowgraphMessage>,
        mut inbox: Receiver<BlockMessage>,
    ) -> Result<(), Error> {
        let TypedBlock {
            meta,
            inputs,
            outputs,
            mio,
            kernel,
        } = self;

        // init work io
        let mut work_io = WorkIo {
            call_again: false,
            finished: false,
            block_on: None,
        };

        // setup phase
        loop {
            match inbox
                .next()
                .await
                .ok_or_else(|| Error::RuntimeError("no msg".to_string()))?
            {
                BlockMessage::Initialize => {
                    if let Err(e) = kernel.init(inputs, outputs, mio, meta).await {
                        error!(
                            "{}: Error during initialization. Terminating.",
                            meta.instance_name().unwrap()
                        );
                        return Err(Error::RuntimeError(e.to_string()));
                    } else {
                        main_inbox
                            .send(FlowgraphMessage::Initialized)
                            .await
                            .map_err(|e| Error::RuntimeError(e.to_string()))?;
                    }
                    break;
                }
                BlockMessage::MessageOutputConnect {
                    src_port,
                    dst_port,
                    dst_inbox,
                } => {
                    mio.output_mut(src_port).connect(dst_port, dst_inbox);
                }
                t => warn!(
                    "{} unhandled message during init {:?}",
                    meta.instance_name().unwrap(),
                    t
                ),
            }
        }

        let inbox = inbox.peekable();
        futures::pin_mut!(inbox);

        // main loop
        loop {
            // ================== non blocking
            loop {
                match inbox.next().now_or_never() {
                    Some(Some(BlockMessage::Notify)) => {}
                    Some(Some(BlockMessage::BlockDescription { tx })) => {
                        let stream_inputs: Vec<String> = self.stream_input_names();
                        let stream_outputs: Vec<String> = self.stream_output_names();
                        let message_inputs: Vec<String> = K::message_input_names()
                            .iter()
                            .map(|x| x.to_string())
                            .collect();
                        let message_outputs: Vec<String> =
                            mio.outputs().iter().map(|x| x.name().to_string()).collect();

                        let description = BlockDescription {
                            id: block_id,
                            type_name: K::type_name().to_string(),
                            instance_name: meta.instance_name().unwrap().to_string(),
                            stream_inputs,
                            stream_outputs,
                            message_inputs,
                            message_outputs,
                            blocking: K::is_blocking(),
                        };
                        tx.send(description).unwrap();
                    }
                    Some(Some(BlockMessage::StreamInputDone { input_id })) => {
                        inputs.finish(input_id)?;
                    }
                    Some(Some(BlockMessage::StreamOutputDone { .. })) => {
                        work_io.finished = true;
                    }
                    Some(Some(BlockMessage::Call { port_id, data })) => {
                        match kernel
                            .call_handler(&mut work_io, mio, meta, port_id, data)
                            .await
                        {
                            Err(Error::InvalidMessagePort(_, port_id)) => {
                                error!(
                                    "{}: BlockMessage::Call -> Invalid Handler {port_id:?}.",
                                    meta.instance_name().unwrap(),
                                );
                            }
                            Err(e @ Error::HandlerError(..)) => {
                                error!(
                                    "{}: BlockMessage::Call -> {e}. Terminating.",
                                    meta.instance_name().unwrap(),
                                );
                                return Err(e);
                            }
                            _ => {}
                        }
                    }
                    Some(Some(BlockMessage::Callback { port_id, data, tx })) => {
                        match kernel
                            .call_handler(&mut work_io, mio, meta, port_id.clone(), data)
                            .await
                        {
                            Err(e @ Error::HandlerError(..)) => {
                                error!(
                                    "{}: BlockMessage::Callback -> {e}. Terminating.",
                                    meta.instance_name().unwrap(),
                                );
                                let _ = tx.send(Err(Error::InvalidMessagePort(
                                    BlockPortCtx::Id(block_id),
                                    port_id,
                                )));
                                return Err(e);
                            }
                            res => {
                                let _ = tx.send(res);
                            }
                        }
                    }
                    Some(Some(BlockMessage::Terminate)) => work_io.finished = true,
                    Some(Some(t)) => warn!("block unhandled message in main loop {:?}", t),
                    _ => break,
                };
                // received at least one message
                work_io.call_again = true;
            }

            // ================== shutdown
            if work_io.finished {
                debug!("{} terminating ", meta.instance_name().unwrap());
                inputs.notify_finished();
                outputs.notify_finished();
                join_all(mio.outputs_mut().iter_mut().map(|o| o.notify_finished())).await;

                match kernel.deinit(inputs, outputs, mio, meta).await {
                    Ok(_) => {
                        break;
                    }
                    Err(e) => {
                        error!(
                            "{}: Error in deinit (). Terminating. ({:?})",
                            meta.instance_name().unwrap_or("<unknown block>"),
                            e
                        );
                        return Err(Error::RuntimeError(e.to_string()));
                    }
                };
            }

            // ================== blocking
            if !work_io.call_again {
                if let Some(f) = work_io.block_on.take() {
                    let p = inbox.as_mut().peek();

                    match futures::future::select(f, p).await {
                        Either::Left(_) => {
                            work_io.call_again = true;
                        }
                        Either::Right((_, f)) => {
                            work_io.block_on = Some(f);
                            continue;
                        }
                    };
                } else {
                    inbox.as_mut().peek().await;
                    continue;
                }
            }

            // ================== work
            work_io.call_again = false;
            if let Err(e) = kernel.work(&mut work_io, inputs, outputs, mio, meta).await {
                error!(
                    "{}: Error in work(). Terminating. ({:?})",
                    meta.instance_name().unwrap(),
                    e
                );
                return Err(Error::RuntimeError(e.to_string()));
            }

            futures_lite::future::yield_now().await;
        }

        Ok(())
    }
}

#[async_trait]
impl<K: KernelInterface + Kernel<I, O> + Send + 'static, I: Send + 'static, O: Send + 'static>
    BlockT for TypedBlock<K, I, O>
{
    // ##### Block
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    // ##### META
    fn instance_name(&self) -> Option<&str> {
        self.meta.instance_name()
    }
    fn set_instance_name(&mut self, name: &str) {
        self.meta.set_instance_name(name)
    }
    fn type_name(&self) -> &str {
        K::type_name()
    }
    fn is_blocking(&self) -> bool {
        K::is_blocking()
    }

    // ##### KERNEL
    async fn run(
        &mut self,
        block_id: usize,
        main_inbox: Sender<FlowgraphMessage>,
        inbox: Receiver<BlockMessage>,
    ) -> Result<(), Error> {
        self.run_impl(block_id, main_inbox, inbox).await
    }

    // ##### STREAM IO
    fn set_tag_propagation(
        &mut self,
        f: Box<dyn FnMut(&mut [StreamInput], &mut [StreamOutput]) + Send + 'static>,
    ) {
        todo!()
    }
    fn stream_input_name_to_id(&self, _name: &str) -> Option<usize> {
        todo!()
    }
    fn stream_output_name_to_id(&self, _name: &str) -> Option<usize> {
        todo!()
    }

    // ##### MESSAGE IO
    fn message_input_name_to_id(&self, name: &str) -> Option<usize> {
        K::message_input_names()
            .iter()
            .enumerate()
            .find(|item| *item.1 == name)
            .map(|(i, _)| i)
    }
    fn message_outputs(&self) -> &Vec<MessageOutput> {
        self.mio.outputs()
    }
    fn message_output_name_to_id(&self, name: &str) -> Option<usize> {
        T::message_output_names()
            .iter()
            .enumerate()
            .find(|item| *item.1 == name)
            .map(|(i, _)| i)
    }
}

/// Block
///
/// Generic wrapper around a [`TypedBlock`].
#[derive(Debug)]
pub struct Block(pub(crate) Box<dyn BlockT>);

impl Block {
    /// Try to cast to a given kernel type
    pub fn kernel<T: Kernel + Send + 'static>(&self) -> Option<&T> {
        self.0
            .as_any()
            .downcast_ref::<TypedBlock<T>>()
            .map(|b| &b.kernel)
    }
    /// Try to mutably cast to a given kernel type
    pub fn kernel_mut<T: Kernel + Send + 'static>(&mut self) -> Option<&T> {
        self.0
            .as_any_mut()
            .downcast_mut::<TypedBlock<T>>()
            .map(|b| &b.kernel)
    }

    // ##### META
    /// Get instance name (see [`BlockMeta::instance_name`])
    pub fn instance_name(&self) -> Option<&str> {
        self.0.instance_name()
    }
    /// Set instance name (see [`BlockMeta::set_instance_name`])
    pub fn set_instance_name(&mut self, name: impl AsRef<str>) {
        self.0.set_instance_name(name.as_ref())
    }
    /// Get type name (see [`BlockMeta::type_name`])
    pub fn type_name(&self) -> &str {
        self.0.type_name()
    }
    /// Is block blocking (see [`BlockMeta::is_blocking`])
    pub fn is_blocking(&self) -> bool {
        self.0.is_blocking()
    }

    pub(crate) async fn run(
        mut self,
        block_id: usize,
        mut main_inbox: Sender<FlowgraphMessage>,
        inbox: Receiver<BlockMessage>,
    ) {
        match self.0.run(block_id, main_inbox.clone(), inbox).await {
            Ok(_) => {
                let _ = main_inbox
                    .send(FlowgraphMessage::BlockDone {
                        block_id,
                        block: self,
                    })
                    .await;
            }
            Err(e) => {
                let instance_name = self
                    .instance_name()
                    .unwrap_or("<broken instance name>")
                    .to_string();
                error!("{}: Error in Block.run() {:?}", instance_name, e);
                let _ = main_inbox
                    .send(FlowgraphMessage::BlockError {
                        block_id,
                        block: self,
                    })
                    .await;
            }
        }
    }

    // ##### STREAM IO
    /// Get stream input ports
    pub fn stream_inputs(&self) -> &Vec<StreamInput> {
        self.0.stream_inputs()
    }
    /// Get stream input port
    pub fn stream_input(&self, id: usize) -> &StreamInput {
        self.0.stream_input(id)
    }
    /// Map stream input port name ot id
    pub fn stream_input_name_to_id(&self, name: &str) -> Option<usize> {
        self.0.stream_input_name_to_id(name)
    }
    /// Get stream output ports
    pub fn stream_outputs(&self) -> &Vec<StreamOutput> {
        self.0.stream_outputs()
    }
    /// Get stream output port
    pub fn stream_output(&self, id: usize) -> &StreamOutput {
        self.0.stream_output(id)
    }
    /// Map stream output port name to id
    pub fn stream_output_name_to_id(&self, name: &str) -> Option<usize> {
        self.0.stream_output_name_to_id(name)
    }

    // ##### MESSAGE IO
    /// Map message input port name to id
    pub fn message_input_name_to_id(&self, name: &str) -> Option<usize> {
        self.0.message_input_name_to_id(name)
    }
    /// Get message output ports
    pub fn message_outputs(&self) -> &Vec<MessageOutput> {
        self.0.message_outputs()
    }
    /// Map message output port name to id
    pub fn message_output_name_to_id(&self, name: &str) -> Option<usize> {
        self.0.message_output_name_to_id(name)
    }
}

impl<K: KernelInterface + Kernel<I, O> + 'static, I: Send + 'static, O: Send + 'static>
    From<TypedBlock<K, I, O>> for Block
{
    fn from(value: TypedBlock<K, I, O>) -> Self {
        Self(Box::new(value))
    }
}

impl fmt::Debug for dyn BlockT {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlockT")
            .field("type_name", &self.type_name().to_string())
            .finish()
    }
}
