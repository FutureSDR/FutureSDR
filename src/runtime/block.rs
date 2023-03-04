use futures::channel::mpsc::{Receiver, Sender};
use futures::future::join_all;
use futures::future::Either;
use futures::prelude::*;
use futures::FutureExt;
use std::any::Any;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

use crate::anyhow::{Context, Result};
use crate::runtime::BlockDescription;
use crate::runtime::BlockMessage;
use crate::runtime::BlockMeta;
use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::MessageIo;
use crate::runtime::MessageOutput;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::StreamInput;
use crate::runtime::StreamIo;
use crate::runtime::StreamOutput;

/// Work IO
///
/// Communicate between `work()` and the runtime.
pub struct WorkIo {
    /// Call block immediately again
    pub call_again: bool,
    /// Mark block as finished
    pub finished: bool,
    /// Block on future
    ///
    /// The block will be called (1) if somehting happens or (2) if the future resolves
    pub block_on: Option<Pin<Box<dyn Future<Output = ()> + Send>>>,
}

impl WorkIo {
    /// Helper to set the future of the Work IO
    pub fn block_on<F: Future<Output = ()> + Send + 'static>(&mut self, f: F) {
        self.block_on = Some(Box::pin(f));
    }
}

impl fmt::Debug for WorkIo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("WorkIo")
            .field("call_again", &self.call_again)
            .field("finished", &self.finished)
            .finish()
    }
}

/// Kernal
///
/// Central trait to implement a block
#[async_trait]
pub trait Kernel: Send {
    /// Processes stream data
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        _s: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        Ok(())
    }
    /// Initialize kernel
    async fn init(
        &mut self,
        _s: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        Ok(())
    }
    /// De-initialize kernel
    async fn deinit(
        &mut self,
        _s: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
pub trait BlockT: Send + Any {
    // ##### BLOCK
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    async fn run(
        &mut self,
        block_id: usize,
        main_inbox: Sender<FlowgraphMessage>,
        inbox: Receiver<BlockMessage>,
    );

    // ##### META
    fn instance_name(&self) -> Option<&str>;
    fn set_instance_name(&mut self, name: &str);
    fn type_name(&self) -> &str;
    fn is_blocking(&self) -> bool;

    // ##### STREAM IO
    #[allow(clippy::type_complexity)]
    fn set_tag_propagation(
        &mut self,
        f: Box<dyn FnMut(&mut [StreamInput], &mut [StreamOutput]) + Send + 'static>,
    );
    fn stream_inputs(&self) -> &Vec<StreamInput>;
    fn stream_input(&self, id: usize) -> &StreamInput;
    fn stream_input_name_to_id(&self, name: &str) -> Option<usize>;
    fn stream_outputs(&self) -> &Vec<StreamOutput>;
    fn stream_output(&self, id: usize) -> &StreamOutput;
    fn stream_output_name_to_id(&self, name: &str) -> Option<usize>;

    // ##### MESSAGE IO
    fn message_input_name_to_id(&self, name: &str) -> Option<usize>;
    fn message_outputs(&self) -> &Vec<MessageOutput>;
    fn message_output_name_to_id(&self, name: &str) -> Option<usize>;
}

/// Typed Block
pub struct TypedBlock<T> {
    /// Block metadata
    pub meta: BlockMeta,
    /// Stream IO
    pub sio: StreamIo,
    /// Message IO
    pub mio: MessageIo<T>,
    /// Kernel
    pub kernel: T,
}

impl<T: Kernel + Send + 'static> TypedBlock<T> {
    /// Create Typed Block
    pub fn new(meta: BlockMeta, sio: StreamIo, mio: MessageIo<T>, kernel: T) -> Self {
        Self {
            meta,
            sio,
            mio,
            kernel,
        }
    }
}

pub(crate) struct TypedBlockWrapper<T> {
    pub(crate) inner: Option<TypedBlock<T>>,
}

impl<T: Kernel + Send + 'static> TypedBlockWrapper<T> {
    async fn call_handler(
        io: &mut WorkIo,
        mio: &mut MessageIo<T>,
        meta: &mut BlockMeta,
        kernel: &mut T,
        id: PortId,
        p: Pmt,
    ) -> std::result::Result<Pmt, Error> {
        let id = match id {
            PortId::Index(i) => {
                if i < mio.inputs().len() {
                    i
                } else {
                    return Err(Error::InvalidHandler(PortId::Index(i)));
                }
            }
            PortId::Name(n) => match mio.input_name_to_id(&n) {
                Some(s) => s,
                None => {
                    return Err(Error::InvalidHandler(PortId::Name(n)));
                }
            },
        };
        if matches!(p, Pmt::Finished) {
            mio.input_mut(id).finish();
        }
        let h = mio.input(id).get_handler();
        let f = (h)(kernel, io, mio, meta, p);
        f.await.or(Err(Error::HandlerError))
    }

    async fn run_impl(
        TypedBlock {
            mut meta,
            mut sio,
            mut mio,
            mut kernel,
        }: TypedBlock<T>,
        block_id: usize,
        mut main_inbox: Sender<FlowgraphMessage>,
        mut inbox: Receiver<BlockMessage>,
    ) -> Result<()> {
        // init work io
        let mut work_io = WorkIo {
            call_again: false,
            finished: false,
            block_on: None,
        };

        // setup phase
        loop {
            match inbox.next().await.context("no msg")? {
                BlockMessage::Initialize => {
                    if let Err(e) = kernel.init(&mut sio, &mut mio, &mut meta).await {
                        error!(
                            "{}: Error during initialization. Terminating.",
                            meta.instance_name().unwrap()
                        );
                        main_inbox
                            .send(FlowgraphMessage::BlockError {
                                block_id,
                                block: Block(Box::new(TypedBlockWrapper {
                                    inner: Some(TypedBlock {
                                        sio,
                                        mio,
                                        meta,
                                        kernel,
                                    }),
                                })),
                            })
                            .await?;
                        return Err(e);
                    } else {
                        main_inbox.send(FlowgraphMessage::Initialized).await?;
                    }
                    break;
                }
                BlockMessage::StreamOutputInit { src_port, writer } => {
                    sio.output(src_port).init(writer);
                }
                BlockMessage::StreamInputInit { dst_port, reader } => {
                    sio.input(dst_port).set_reader(reader);
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
                        let stream_inputs: Vec<String> =
                            sio.inputs().iter().map(|x| x.name().to_string()).collect();
                        let stream_outputs: Vec<String> =
                            sio.outputs().iter().map(|x| x.name().to_string()).collect();
                        let message_inputs: Vec<String> = mio.input_names();
                        let message_outputs: Vec<String> =
                            mio.outputs().iter().map(|x| x.name().to_string()).collect();

                        let description = BlockDescription {
                            id: block_id,
                            type_name: meta.type_name().to_string(),
                            instance_name: meta.instance_name().unwrap().to_string(),
                            stream_inputs,
                            stream_outputs,
                            message_inputs,
                            message_outputs,
                            blocking: meta.is_blocking(),
                        };
                        tx.send(description).unwrap();
                    }
                    Some(Some(BlockMessage::StreamInputDone { input_id })) => {
                        sio.input(input_id).finish();
                    }
                    Some(Some(BlockMessage::StreamOutputDone { .. })) => {
                        work_io.finished = true;
                    }
                    Some(Some(BlockMessage::Call { port_id, data })) => {
                        match Self::call_handler(
                            &mut work_io,
                            &mut mio,
                            &mut meta,
                            &mut kernel,
                            port_id,
                            data,
                        )
                        .await
                        {
                            Err(Error::InvalidHandler(port_id)) => {
                                error!(
                                    "{}: BlockMessage::Call -> Invalid Handler {port_id:?}.",
                                    meta.instance_name().unwrap(),
                                );
                            }
                            Err(Error::HandlerError) => {
                                error!(
                                    "{}: BlockMessage::Call -> HandlerError. Terminating.",
                                    meta.instance_name().unwrap(),
                                );
                                main_inbox
                                    .send(FlowgraphMessage::BlockError {
                                        block_id,
                                        block: Block(Box::new(TypedBlockWrapper {
                                            inner: Some(TypedBlock {
                                                sio,
                                                mio,
                                                meta,
                                                kernel,
                                            }),
                                        })),
                                    })
                                    .await?;
                                return Err(Error::HandlerError.into());
                            }
                            _ => {}
                        }
                    }
                    Some(Some(BlockMessage::Callback { port_id, data, tx })) => {
                        match Self::call_handler(
                            &mut work_io,
                            &mut mio,
                            &mut meta,
                            &mut kernel,
                            port_id.clone(),
                            data,
                        )
                        .await
                        {
                            Err(Error::HandlerError) => {
                                error!(
                                    "{}: Error in callback. Terminating.",
                                    meta.instance_name().unwrap(),
                                );
                                let _ = tx.send(Err(Error::InvalidHandler(port_id)));
                                main_inbox
                                    .send(FlowgraphMessage::BlockError {
                                        block_id,
                                        block: Block(Box::new(TypedBlockWrapper {
                                            inner: Some(TypedBlock {
                                                sio,
                                                mio,
                                                meta,
                                                kernel,
                                            }),
                                        })),
                                    })
                                    .await?;
                                return Err(Error::HandlerError.into());
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
                join_all(sio.inputs_mut().iter_mut().map(|i| i.notify_finished())).await;
                join_all(sio.outputs_mut().iter_mut().map(|o| o.notify_finished())).await;
                join_all(mio.outputs_mut().iter_mut().map(|o| o.notify_finished())).await;

                match kernel.deinit(&mut sio, &mut mio, &mut meta).await {
                    Ok(_) => {
                        let _ = main_inbox
                            .send(FlowgraphMessage::BlockDone {
                                block_id,
                                block: Block(Box::new(TypedBlockWrapper {
                                    inner: Some(TypedBlock {
                                        sio,
                                        mio,
                                        meta,
                                        kernel,
                                    }),
                                })),
                            })
                            .await;
                        break;
                    }
                    Err(e) => {
                        error!(
                            "{}: Error in deinit (). Terminating. ({:?})",
                            meta.instance_name().unwrap(),
                            e
                        );
                        main_inbox
                            .send(FlowgraphMessage::BlockError {
                                block_id,
                                block: Block(Box::new(TypedBlockWrapper {
                                    inner: Some(TypedBlock {
                                        sio,
                                        mio,
                                        meta,
                                        kernel,
                                    }),
                                })),
                            })
                            .await?;
                        return Err(e);
                    }
                };
            }

            // ================== blocking
            if !work_io.call_again {
                if let Some(f) = work_io.block_on.take() {
                    let p = inbox.as_mut().peek();

                    match future::select(f, p).await {
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
            if let Err(e) = kernel
                .work(&mut work_io, &mut sio, &mut mio, &mut meta)
                .await
            {
                error!(
                    "{}: Error in work(). Terminating. ({:?})",
                    meta.instance_name().unwrap(),
                    e
                );
                main_inbox
                    .send(FlowgraphMessage::BlockError {
                        block_id,
                        block: Block(Box::new(TypedBlockWrapper {
                            inner: Some(TypedBlock {
                                sio,
                                mio,
                                meta,
                                kernel,
                            }),
                        })),
                    })
                    .await?;
                return Err(e);
            }
            sio.commit();

            futures_lite::future::yield_now().await;
        }

        Ok(())
    }
}

#[async_trait]
impl<T: Kernel + Send + 'static> BlockT for TypedBlockWrapper<T> {
    // ##### Block
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    // ##### META
    fn instance_name(&self) -> Option<&str> {
        self.inner.as_ref().and_then(|i| i.meta.instance_name())
    }
    fn set_instance_name(&mut self, name: &str) {
        if let Some(i) = self.inner.as_mut() {
            i.meta.set_instance_name(name)
        }
    }
    fn type_name(&self) -> &str {
        self.inner.as_ref().map(|i| i.meta.type_name()).unwrap()
    }
    fn is_blocking(&self) -> bool {
        self.inner.as_ref().map(|i| i.meta.is_blocking()).unwrap()
    }

    // ##### KERNEL
    async fn run(
        &mut self,
        block_id: usize,
        main_inbox: Sender<FlowgraphMessage>,
        inbox: Receiver<BlockMessage>,
    ) {
        let block = self.inner.take().unwrap();

        let instance_name = block
            .meta
            .instance_name()
            .unwrap_or("<broken instance name>")
            .to_string();
        if let Err(e) = Self::run_impl(block, block_id, main_inbox, inbox).await {
            error!("{}: Error in Block.run() {:?}", instance_name, e);
        }
    }

    // ##### STREAM IO
    fn set_tag_propagation(
        &mut self,
        f: Box<dyn FnMut(&mut [StreamInput], &mut [StreamOutput]) + Send + 'static>,
    ) {
        if let Some(i) = self.inner.as_mut() {
            i.sio.set_tag_propagation(f)
        }
    }
    fn stream_inputs(&self) -> &Vec<StreamInput> {
        self.inner.as_ref().map(|i| i.sio.inputs()).unwrap()
    }
    fn stream_input(&self, id: usize) -> &StreamInput {
        self.inner.as_ref().map(|i| i.sio.input_ref(id)).unwrap()
    }
    fn stream_input_name_to_id(&self, name: &str) -> Option<usize> {
        self.inner
            .as_ref()
            .map(|i| i.sio.input_name_to_id(name))
            .unwrap()
    }
    fn stream_outputs(&self) -> &Vec<StreamOutput> {
        self.inner.as_ref().map(|i| i.sio.outputs()).unwrap()
    }
    fn stream_output(&self, id: usize) -> &StreamOutput {
        self.inner.as_ref().map(|i| i.sio.output_ref(id)).unwrap()
    }
    fn stream_output_name_to_id(&self, name: &str) -> Option<usize> {
        self.inner
            .as_ref()
            .map(|i| i.sio.output_name_to_id(name))
            .unwrap()
    }

    // ##### MESSAGE IO
    fn message_input_name_to_id(&self, name: &str) -> Option<usize> {
        self.inner
            .as_ref()
            .map(|i| i.mio.input_name_to_id(name))
            .unwrap()
    }
    fn message_outputs(&self) -> &Vec<MessageOutput> {
        self.inner.as_ref().map(|i| i.mio.outputs()).unwrap()
    }
    fn message_output_name_to_id(&self, name: &str) -> Option<usize> {
        self.inner
            .as_ref()
            .map(|i| i.mio.output_name_to_id(name))
            .unwrap()
    }
}

/// Block
///
/// Generic wrapper around a [`TypedBlock`].
#[derive(Debug)]
pub struct Block(pub(crate) Box<dyn BlockT>);

impl Block {
    /// Create Block
    pub fn new<T: Kernel + Send + 'static>(
        meta: BlockMeta,
        sio: StreamIo,
        mio: MessageIo<T>,
        kernel: T,
    ) -> Block {
        Self(Box::new(TypedBlockWrapper {
            inner: Some(TypedBlock {
                meta,
                sio,
                mio,
                kernel,
            }),
        }))
    }
    /// Create block by wrapping a [`TypedBlock`].
    pub fn from_typed<T: Kernel + Send + 'static>(b: TypedBlock<T>) -> Block {
        Self(Box::new(TypedBlockWrapper { inner: Some(b) }))
    }
    /// Try to cast to a given kernel type
    pub fn kernel<T: Kernel + Send + 'static>(&self) -> Option<&T> {
        self.0
            .as_any()
            .downcast_ref::<TypedBlockWrapper<T>>()
            .and_then(|b| b.inner.as_ref())
            .map(|b| &b.kernel)
    }
    /// Try to mutably cast to a given kernel type
    pub fn kernel_mut<T: Kernel + Send + 'static>(&mut self) -> Option<&T> {
        self.0
            .as_any_mut()
            .downcast_mut::<TypedBlockWrapper<T>>()
            .and_then(|b| b.inner.as_mut())
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
        main_inbox: Sender<FlowgraphMessage>,
        inbox: Receiver<BlockMessage>,
    ) {
        self.0.run(block_id, main_inbox, inbox).await
    }

    // ##### STREAM IO
    /// Set tag propagation function
    #[allow(clippy::type_complexity)]
    pub fn set_tag_propagation(
        &mut self,
        f: Box<dyn FnMut(&mut [StreamInput], &mut [StreamOutput]) + Send + 'static>,
    ) {
        self.0.set_tag_propagation(f);
    }
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

impl<T: Kernel + Send + 'static> fmt::Debug for TypedBlockWrapper<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AsyncBlock")
            .field("type_name", &self.type_name().to_string())
            .finish()
    }
}

impl fmt::Debug for dyn BlockT {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlockT")
            .field("type_name", &self.type_name().to_string())
            .finish()
    }
}
