use futures::future::Either;
use futures::FutureExt;
use futures::SinkExt;
use futures::StreamExt;
use std::any::Any;
use std::fmt;
use std::ops::Deref;
use std::ops::DerefMut;

use futuresdr::channel::mpsc;
use futuresdr::channel::mpsc::Sender;
use futuresdr::runtime::config;
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
use futuresdr::runtime::Result;
use futuresdr::runtime::WorkIo;

#[async_trait]
/// Block interface, implemented for [WrappedKernel]s
pub trait Block: Send + Any {
    // ##### BLOCK
    /// Cast block to [std::any::Any].
    fn as_any(&self) -> &dyn Any;
    /// Cast block to [std::any::Any] mutably.
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// Run the block.
    async fn run(&mut self, main_inbox: Sender<FlowgraphMessage>) -> Result<(), Error>;
    /// Get the inbox of the block
    fn inbox(&self) -> Sender<BlockMessage>;
    /// Get the ID of the block
    fn id(&self) -> BlockId;

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
    pub fn new(kernel: K, id: BlockId) -> Self {
        let (tx, rx) = mpsc::channel(config::config().queue_size);
        Self {
            meta: BlockMeta::new(),
            mio: MessageOutputs::new(
                K::message_output_names()
                    .iter()
                    .map(|x| x.to_string())
                    .collect(),
            ),
            kernel,
            id,
            inbox: rx,
            inbox_tx: tx,
        }
    }

    async fn run_impl(&mut self, mut main_inbox: Sender<FlowgraphMessage>) -> Result<(), Error> {
        let WrappedKernel {
            meta,
            mio,
            kernel,
            inbox,
            ..
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
                    if let Err(e) = kernel.init(mio, meta).await {
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
                        let stream_inputs =
                            K::stream_inputs().iter().map(|n| n.to_string()).collect();
                        let stream_outputs =
                            K::stream_outputs().iter().map(|n| n.to_string()).collect();
                        let message_inputs =
                            K::message_inputs().iter().map(|n| n.to_string()).collect();
                        let message_outputs =
                            K::message_outputs().iter().map(|n| n.to_string()).collect();

                        let description = BlockDescription {
                            id: self.id,
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
                        kernel.stream_input_finish(input_id)?;
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
                                    BlockPortCtx::Id(self.id.0),
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
                kernel.stream_ports_notify_finished().await;
                mio.notify_finished().await;

                match kernel.deinit(mio, meta).await {
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
            if let Err(e) = kernel.work(&mut work_io, mio, meta).await {
                error!(
                    "{}: Error in work(). Terminating. ({:?})",
                    meta.instance_name().unwrap(),
                    e
                );
                return Err(Error::RuntimeError(e.to_string()));
            }

            futuresdr::runtime::futures::yield_now().await;
        }

        Ok(())
    }
}

#[async_trait]
impl<K: KernelInterface + Kernel + Send + 'static> Block for WrappedKernel<K> {
    // ##### Block
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn inbox(&self) -> Sender<BlockMessage> {
        self.inbox_tx.clone()
    }
    fn id(&self) -> BlockId {
        self.block_id
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
    async fn run(&mut self, main_inbox: Sender<FlowgraphMessage>) -> Result<(), Error> {
        self.run_impl(main_inbox).await
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
