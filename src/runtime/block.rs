use futures::SinkExt;
use futures::StreamExt;
use futures::future::Either;
use std::any::Any;
use std::fmt;
use std::ops::Deref;
use std::ops::DerefMut;

use futuresdr::channel::mpsc;
use futuresdr::channel::mpsc::Sender;
use futuresdr::runtime::BlockDescription;
use futuresdr::runtime::BlockId;
use futuresdr::runtime::BlockMessage;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockPortCtx;
use futuresdr::runtime::Error;
use futuresdr::runtime::FlowgraphMessage;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::KernelInterface;
use futuresdr::runtime::MessageOutputs;
use futuresdr::runtime::PortId;
use futuresdr::runtime::Result;
use futuresdr::runtime::WorkIo;
use futuresdr::runtime::buffer::BufferReader;
use futuresdr::runtime::config;

#[async_trait]
/// Block interface, implemented for [WrappedKernel]s
pub trait Block: Send + Any {
    /// required for downcasting
    fn as_any_mut(&mut self) -> &mut dyn Any;

    // ##### BLOCK
    /// Run the block.
    async fn run(&mut self, main_inbox: Sender<FlowgraphMessage>);
    /// Get the inbox of the block
    fn inbox(&self) -> Sender<BlockMessage>;
    /// Get the ID of the block
    fn id(&self) -> BlockId;

    // ##### Stream Ports
    /// Get dyn reference to stream input
    fn stream_input(&mut self, name: &str) -> Option<&mut dyn BufferReader>;
    /// Connect dyn BufferReader by downcasting it
    fn connect_stream_output(
        &mut self,
        name: &str,
        reader: &mut dyn BufferReader,
    ) -> Result<(), Error>;

    // ##### Message Ports
    /// Message inputs of the block
    fn message_inputs(&self) -> &'static [&'static str];
    /// Connect message output port
    fn connect(
        &mut self,
        src_port: &PortId,
        sender: Sender<BlockMessage>,
        dst_port: &PortId,
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
}

impl fmt::Debug for dyn Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Block")
            .field("type_name", &self.type_name().to_string())
            .finish()
    }
}
/// Typed Block
pub struct WrappedKernel<K: Kernel> {
    /// Block metadata
    pub meta: BlockMeta,
    /// Message IO
    pub mio: MessageOutputs,
    /// Kernel
    pub kernel: K,
    /// Block ID
    pub id: BlockId,
    /// Inbox for Actor Model
    pub inbox: mpsc::Receiver<BlockMessage>,
    /// Sending-side of Inbox
    pub inbox_tx: mpsc::Sender<BlockMessage>,
}

impl<K: KernelInterface + Kernel + Send + 'static> WrappedKernel<K> {
    /// Create Typed Block
    pub fn new(mut kernel: K, id: BlockId) -> Self {
        let (tx, rx) = mpsc::channel(config::config().queue_size);
        kernel.stream_ports_init(id, tx.clone());
        Self {
            meta: BlockMeta::new(),
            mio: MessageOutputs::new(
                id,
                K::message_outputs().iter().map(|x| x.to_string()).collect(),
            ),
            kernel,
            id,
            inbox: rx,
            inbox_tx: tx,
        }
    }

    async fn run_impl(&mut self, mut main_inbox: Sender<FlowgraphMessage>) -> Result<(), Error> {
        let instance_name = self.instance_name().unwrap_or(self.type_name()).to_owned();
        let WrappedKernel {
            meta,
            mio,
            kernel,
            inbox,
            ..
        } = self;

        kernel.stream_ports_validate()?;

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
                    match kernel.init(mio, meta).await {
                        Err(e) => {
                            error!(
                                "{}: Error during initialization. Terminating.",
                                instance_name
                            );
                            return Err(Error::RuntimeError(e.to_string()));
                        }
                        _ => {
                            main_inbox
                                .send(FlowgraphMessage::Initialized)
                                .await
                                .map_err(|e| Error::RuntimeError(e.to_string()))?;
                        }
                    }
                    break;
                }
                t => warn!("{} unhandled message during init {:?}", instance_name, t),
            }
        }

        let mut peek: Option<BlockMessage> = None;

        // main loop
        loop {
            // ================== non blocking
            let mut msg = peek.take().or_else(|| inbox.try_recv().ok());
            while let Some(m) = msg {
                match m {
                    BlockMessage::Notify => {}
                    BlockMessage::BlockDescription { tx } => {
                        let stream_inputs = kernel.stream_inputs();
                        let stream_outputs = kernel.stream_outputs();
                        let message_inputs =
                            K::message_inputs().iter().map(|n| n.to_string()).collect();
                        let message_outputs =
                            K::message_outputs().iter().map(|n| n.to_string()).collect();

                        let description = BlockDescription {
                            id: self.id,
                            type_name: K::type_name().to_string(),
                            instance_name: instance_name.clone(),
                            stream_inputs,
                            stream_outputs,
                            message_inputs,
                            message_outputs,
                            blocking: K::is_blocking(),
                        };
                        if tx.send(description).is_err() {
                            warn!("failed to return BlockDescription, oneshot receiver dropped");
                        }
                    }
                    BlockMessage::StreamInputDone { input_id } => {
                        kernel.stream_input_finish(input_id)?;
                    }
                    BlockMessage::StreamOutputDone { .. } => {
                        work_io.finished = true;
                    }
                    BlockMessage::Call { port_id, data } => {
                        match kernel
                            .call_handler(&mut work_io, mio, meta, port_id, data)
                            .await
                        {
                            Err(Error::InvalidMessagePort(_, port_id)) => {
                                error!(
                                    "{}: BlockMessage::Call -> Invalid Handler {port_id:?}.",
                                    instance_name
                                );
                            }
                            Err(e @ Error::HandlerError(..)) => {
                                error!(
                                    "{}: BlockMessage::Call -> {e}. Terminating.",
                                    instance_name
                                );
                                return Err(e);
                            }
                            _ => {}
                        }
                    }
                    BlockMessage::Callback { port_id, data, tx } => {
                        match kernel
                            .call_handler(&mut work_io, mio, meta, port_id.clone(), data)
                            .await
                        {
                            Err(e @ Error::HandlerError(..)) => {
                                error!(
                                    "{}: BlockMessage::Callback -> {e}. Terminating.",
                                    instance_name
                                );
                                let _ = tx.send(Err(Error::InvalidMessagePort(
                                    BlockPortCtx::Id(self.id),
                                    port_id,
                                )));
                                return Err(e);
                            }
                            res => {
                                let _ = tx.send(res);
                            }
                        }
                    }
                    BlockMessage::Terminate => work_io.finished = true,
                    t => warn!("block unhandled message in main loop {:?}", t),
                };
                // received at least one message
                work_io.call_again = true;
                msg = inbox.try_recv().ok();
            }

            // ================== shutdown
            if work_io.finished {
                debug!("{} terminating ", instance_name);
                kernel.stream_ports_notify_finished().await;
                mio.notify_finished().await;

                match kernel.deinit(mio, meta).await {
                    Ok(_) => {
                        break;
                    }
                    Err(e) => {
                        error!(
                            "{}: Error in deinit (). Terminating. ({:?})",
                            instance_name, e
                        );
                        return Err(Error::RuntimeError(e.to_string()));
                    }
                };
            }

            // ================== blocking
            if !work_io.call_again {
                match work_io.block_on.take() {
                    Some(f) => {
                        let p = inbox.next();

                        match futures::future::select(f, p).await {
                            Either::Left(_) => {
                                work_io.call_again = true;
                            }
                            Either::Right((p, f)) => {
                                peek = p;
                                work_io.block_on = Some(f);
                                continue;
                            }
                        };
                    }
                    _ => {
                        peek = inbox.next().await;
                        continue;
                    }
                }
            }

            // ================== work
            work_io.call_again = false;
            if let Err(e) = kernel.work(&mut work_io, mio, meta).await {
                error!("{}: Error in work(). Terminating. ({:?})", instance_name, e);
                return Err(Error::RuntimeError(e.to_string()));
            }

            futuresdr::runtime::futures::yield_now().await;
        }

        Ok(())
    }
}

#[async_trait]
impl<K: KernelInterface + Kernel + Send + 'static> Block for WrappedKernel<K> {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    // ##### Block
    fn inbox(&self) -> Sender<BlockMessage> {
        self.inbox_tx.clone()
    }
    fn id(&self) -> BlockId {
        self.id
    }

    // ##### Stream Ports
    fn stream_input(&mut self, name: &str) -> Option<&mut dyn BufferReader> {
        self.kernel.stream_input(name)
    }
    fn connect_stream_output(
        &mut self,
        name: &str,
        reader: &mut dyn BufferReader,
    ) -> Result<(), Error> {
        self.kernel.connect_stream_output(name, reader)
    }

    // ##### Message Ports
    fn message_inputs(&self) -> &'static [&'static str] {
        K::message_inputs()
    }
    fn connect(
        &mut self,
        src_port: &PortId,
        dst_box: Sender<BlockMessage>,
        dst_port: &PortId,
    ) -> Result<(), Error> {
        self.mio.connect(src_port, dst_box, dst_port)
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
    async fn run(&mut self, mut main_inbox: Sender<FlowgraphMessage>) {
        match self.run_impl(main_inbox.clone()).await {
            Ok(_) => {
                let _ = main_inbox
                    .send(FlowgraphMessage::BlockDone {
                        block_id: self.id(),
                    })
                    .await;
                return;
            }
            Err(e) => {
                let instance_name = self
                    .instance_name()
                    .unwrap_or("<instance name not set>")
                    .to_string();
                error!("{}: Error in Block.run() {:?}", instance_name, e);
                let _ = main_inbox
                    .send(FlowgraphMessage::BlockError {
                        block_id: self.id(),
                    })
                    .await;
            }
        }
    }
}

impl<K: Kernel> Deref for WrappedKernel<K> {
    type Target = K;

    fn deref(&self) -> &Self::Target {
        &self.kernel
    }
}
impl<K: Kernel> DerefMut for WrappedKernel<K> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.kernel
    }
}
