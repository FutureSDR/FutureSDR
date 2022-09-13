#[cfg(not(target_arch = "wasm32"))]
use async_io::block_on;
#[cfg(not(target_arch = "wasm32"))]
use async_task::Task;
use futures::channel::mpsc::{channel, Receiver, Sender};
use futures::channel::oneshot;
use futures::future::join_all;
use futures::future::Either;
use futures::prelude::*;
use futures::FutureExt;
#[cfg(target_arch = "wasm32")]
type Task<T> = crate::runtime::scheduler::wasm::TaskHandle<T>;

use crate::anyhow::{bail, Context, Result};
use crate::runtime::config;
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::ctrl_port;
use crate::runtime::scheduler::Scheduler;
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::scheduler::SmolScheduler;
#[cfg(target_arch = "wasm32")]
use crate::runtime::scheduler::WasmScheduler;
use crate::runtime::Block;
use crate::runtime::BlockDescription;
use crate::runtime::BlockMessage;
use crate::runtime::Flowgraph;
use crate::runtime::FlowgraphDescription;
use crate::runtime::FlowgraphHandle;
use crate::runtime::FlowgraphMessage;
use crate::runtime::WorkIo;

/// This is the [Runtime] that runs a [Flowgraph] to completion.
///
/// [Runtime]s are generic over the scheduler used to run the [Flowgraph].
pub struct Runtime<S> {
    scheduler: S,
}

#[cfg(not(target_arch = "wasm32"))]
impl Runtime<SmolScheduler> {
    /// Constructs a new [Runtime] using [SmolScheduler::default()] for the [Scheduler].
    pub fn new() -> Runtime<SmolScheduler> {
        RuntimeBuilder {
            scheduler: SmolScheduler::default(),
        }
        .build()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for Runtime<SmolScheduler> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_arch = "wasm32")]
impl Runtime<WasmScheduler> {
    pub fn new() -> Runtime<WasmScheduler> {
        RuntimeBuilder {
            scheduler: WasmScheduler::default(),
        }
        .build()
    }
}

#[cfg(target_arch = "wasm32")]
impl Default for Runtime<WasmScheduler> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Scheduler> Runtime<S> {
    /// Create a [Runtime] with a given [Scheduler]
    pub fn with_scheduler(scheduler: S) -> Runtime<S> {
        RuntimeBuilder { scheduler }.build()
    }

    pub fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        self.scheduler.spawn(future)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn block_on<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> T {
        block_on(self.scheduler.spawn(future))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn_background<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) {
        self.scheduler.spawn(future).detach();
    }

    pub fn spawn_blocking<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        self.scheduler.spawn_blocking(future)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn_blocking_background<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) {
        self.scheduler.spawn_blocking(future).detach();
    }

    pub async fn start(&self, fg: Flowgraph) -> (Task<Result<Flowgraph>>, FlowgraphHandle) {
        let queue_size = config::config().queue_size;
        let (fg_inbox, fg_inbox_rx) = channel::<FlowgraphMessage>(queue_size);

        let (tx, rx) = oneshot::channel::<()>();
        let task = self.scheduler.spawn(run_flowgraph(
            fg,
            self.scheduler.clone(),
            fg_inbox.clone(),
            fg_inbox_rx,
            tx,
        ));
        rx.await
            .expect("run_flowgraph did not signal startup completed");
        (task, FlowgraphHandle::new(fg_inbox))
    }

    /// Main method that kicks-off the running of a [Flowgraph].
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run(&self, fg: Flowgraph) -> Result<Flowgraph> {
        let (handle, _) = block_on(self.start(fg));
        block_on(handle)
    }

    pub async fn run_async(&self, fg: Flowgraph) -> Result<Flowgraph> {
        let (handle, _) = self.start(fg).await;
        handle.await
    }
}

pub struct RuntimeBuilder<S> {
    scheduler: S,
}

impl<S: Scheduler> RuntimeBuilder<S> {
    pub fn build(self) -> Runtime<S> {
        crate::runtime::init();
        Runtime {
            scheduler: self.scheduler,
        }
    }
}

async fn run_flowgraph<S: Scheduler>(
    mut fg: Flowgraph,
    scheduler: S,
    mut main_channel: Sender<FlowgraphMessage>,
    mut main_rx: Receiver<FlowgraphMessage>,
    initialized: oneshot::Sender<()>,
) -> Result<Flowgraph> {
    debug!("in run_flowgraph");
    let mut topology = fg.topology.take().context("flowgraph not initialized")?;
    topology.validate()?;

    let mut inboxes = scheduler.run_topology(&mut topology, &main_channel);

    debug!("connect stream io");
    // connect stream IO
    for ((src, src_port, buffer_builder), v) in topology.stream_edges.iter() {
        debug_assert!(!v.is_empty());

        let src_inbox = inboxes[*src].as_ref().unwrap().clone();
        let mut writer = buffer_builder.build(src_inbox, *src_port);

        for (dst, dst_port) in v.iter() {
            let dst_inbox = inboxes[*dst].as_ref().unwrap().clone();

            inboxes[*dst]
                .as_mut()
                .unwrap()
                .send(BlockMessage::StreamInputInit {
                    dst_port: *dst_port,
                    reader: writer.add_reader(dst_inbox, *dst_port),
                })
                .await
                .unwrap();
        }

        inboxes[*src]
            .as_mut()
            .unwrap()
            .send(BlockMessage::StreamOutputInit {
                src_port: *src_port,
                writer,
            })
            .await
            .unwrap();
    }

    debug!("connect message io");
    // connect message IO
    for (src, src_port, dst, dst_port) in topology.message_edges.iter() {
        let dst_box = inboxes[*dst].as_ref().unwrap().clone();
        inboxes[*src]
            .as_mut()
            .unwrap()
            .send(BlockMessage::MessageOutputConnect {
                src_port: *src_port,
                dst_port: *dst_port,
                dst_inbox: dst_box,
            })
            .await
            .unwrap();
    }

    debug!("init blocks");
    // init blocks
    let mut active_blocks = 0u32;
    for (_, opt) in inboxes.iter_mut() {
        if let Some(ref mut chan) = opt {
            chan.send(BlockMessage::Initialize).await.unwrap();
            active_blocks += 1;
        }
    }

    debug!("wait for blocks init");
    // wait until all blocks are initialized
    let mut i = active_blocks;
    let mut queue = Vec::new();
    loop {
        if i == 0 {
            break;
        }

        let m = main_rx.next().await.context("no msg")?;
        match m {
            FlowgraphMessage::Initialized => i -= 1,
            x => {
                debug!(
                    "queueing unhandled message received during initialization {:?}",
                    &x
                );
                queue.push(x);
            }
        }
    }

    debug!("running blocks");
    for (_, opt) in inboxes.iter_mut() {
        if let Some(ref mut chan) = opt {
            if chan.send(BlockMessage::Notify).await.is_err() {
                debug!("runtime wanted to start block that already terminated");
            }
        }
    }

    for m in queue.into_iter() {
        main_channel
            .try_send(m)
            .expect("main inbox exceeded capacity during startup");
    }

    // Start Control Port
    #[cfg(not(target_arch = "wasm32"))]
    {
        let routes = fg.custom_routes.clone();
        ctrl_port::start_control_port(FlowgraphHandle::new(main_channel.clone()), routes).await;
    }

    initialized
        .send(())
        .expect("failed to signal flowgraph startup complete.");

    // main loop
    loop {
        if active_blocks == 0 {
            break;
        }

        let m = main_rx.next().await.context("no msg")?;
        match m {
            FlowgraphMessage::BlockCall {
                block_id,
                port_id,
                data,
            } => {
                inboxes[block_id]
                    .as_mut()
                    .unwrap()
                    .send(BlockMessage::Call { port_id, data })
                    .await
                    .unwrap();
            }
            FlowgraphMessage::BlockCallback {
                block_id,
                port_id,
                data,
                tx,
            } => {
                inboxes[block_id]
                    .as_mut()
                    .unwrap()
                    .send(BlockMessage::Callback { port_id, data, tx })
                    .await
                    .unwrap();
            }
            FlowgraphMessage::BlockDone { block_id, block } => {
                *topology.blocks.get_mut(block_id).unwrap() = Some(block);

                active_blocks -= 1;
            }
            FlowgraphMessage::BlockDescription { block_id, tx } => {
                inboxes[block_id]
                    .as_mut()
                    .unwrap()
                    .send(BlockMessage::BlockDescription { tx })
                    .await
                    .unwrap();
            }
            FlowgraphMessage::FlowgraphDescription { tx } => {
                let mut blocks = Vec::new();
                let ids: Vec<usize> = topology.blocks.iter().map(|x| x.0).collect();
                for id in ids {
                    let (b_tx, rx) = oneshot::channel::<BlockDescription>();
                    inboxes[id]
                        .as_mut()
                        .unwrap()
                        .send(BlockMessage::BlockDescription { tx: b_tx })
                        .await
                        .unwrap();
                    blocks.push(rx.await.unwrap());
                }

                let stream_edges = topology
                    .stream_edges
                    .iter()
                    .flat_map(|x| x.1.iter().map(|y| (x.0 .0, x.0 .1, y.0, y.1)))
                    .collect();
                let message_edges = topology.message_edges.clone();

                tx.send(FlowgraphDescription {
                    blocks,
                    stream_edges,
                    message_edges,
                })
                .unwrap();
            }
            FlowgraphMessage::Terminate => {
                for (_, opt) in inboxes.iter_mut() {
                    if let Some(ref mut chan) = opt {
                        if chan.send(BlockMessage::Terminate).await.is_err() {
                            debug!("runtime tried to terminate block that was already terminated");
                        }
                    }
                }
                bail!("Flowgraph was terminated");
            }
            _ => warn!("main loop received unhandled message"),
        }
    }

    fg.topology = Some(topology);
    Ok(fg)
}

pub(crate) async fn run_block(
    mut block: Block,
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
                if let Err(e) = block.init().await {
                    error!(
                        "{}: Error during initialization. Terminating.",
                        block.instance_name().unwrap()
                    );
                    main_inbox.send(FlowgraphMessage::Terminate).await?;
                    return Err(e);
                } else {
                    main_inbox.send(FlowgraphMessage::Initialized).await?;
                }
                break;
            }
            BlockMessage::StreamOutputInit { src_port, writer } => {
                block.stream_output_mut(src_port).init(writer);
            }
            BlockMessage::StreamInputInit { dst_port, reader } => {
                block.stream_input_mut(dst_port).set_reader(reader);
            }
            BlockMessage::MessageOutputConnect {
                src_port,
                dst_port,
                dst_inbox,
            } => {
                block
                    .message_output_mut(src_port)
                    .connect(dst_port, dst_inbox);
            }
            t => warn!(
                "{} unhandled message during init {:?}",
                block.instance_name().unwrap(),
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
                    let stream_inputs: Vec<String> = block
                        .stream_inputs()
                        .iter()
                        .map(|x| x.name().to_string())
                        .collect();
                    let stream_outputs: Vec<String> = block
                        .stream_outputs()
                        .iter()
                        .map(|x| x.name().to_string())
                        .collect();
                    let message_inputs: Vec<String> = block.message_input_names();
                    let message_outputs: Vec<String> = block
                        .message_outputs()
                        .iter()
                        .map(|x| x.name().to_string())
                        .collect();

                    let description = BlockDescription {
                        id: block_id,
                        type_name: block.type_name().to_string(),
                        instance_name: block.instance_name().unwrap().to_string(),
                        stream_inputs,
                        stream_outputs,
                        message_inputs,
                        message_outputs,
                        blocking: block.is_blocking(),
                    };
                    tx.send(description).unwrap();
                }
                Some(Some(BlockMessage::StreamInputDone { input_id })) => {
                    block.stream_input_mut(input_id).finish();
                }
                Some(Some(BlockMessage::StreamOutputDone { .. })) => {
                    work_io.finished = true;
                }
                Some(Some(BlockMessage::Call { port_id, data })) => {
                    if let Err(e) = block.call_handler(port_id, data).await {
                        error!(
                            "{}: Error in callback. Terminating. ({:?})",
                            block.instance_name().unwrap(),
                            e
                        );
                        main_inbox.send(FlowgraphMessage::Terminate).await?;
                        return Err(e);
                    }
                }
                Some(Some(BlockMessage::Callback { port_id, data, tx })) => {
                    match block.call_handler(port_id, data).await {
                        Ok(res) => {
                            tx.send(res).unwrap();
                        }
                        Err(e) => {
                            error!(
                                "{}: Error in callback. Terminating. ({:?})",
                                block.instance_name().unwrap(),
                                e
                            );
                            main_inbox.send(FlowgraphMessage::Terminate).await?;
                            return Err(e);
                        }
                    }
                }
                Some(Some(BlockMessage::Terminate)) => work_io.finished = true,
                Some(Some(t)) => warn!("block unhandled message in main loop {:?}", t),
                _ => break,
            }
            // received at least one message
            work_io.call_again = true;
        }

        // ================== shutdown
        if work_io.finished {
            debug!("{} terminating ", block.instance_name().unwrap());
            join_all(
                block
                    .stream_inputs_mut()
                    .iter_mut()
                    .map(|i| i.notify_finished()),
            )
            .await;
            join_all(
                block
                    .stream_outputs_mut()
                    .iter_mut()
                    .map(|o| o.notify_finished()),
            )
            .await;
            join_all(
                block
                    .message_outputs_mut()
                    .iter_mut()
                    .map(|o| o.notify_finished()),
            )
            .await;

            block.deinit().await?;

            // ============= notify main thread
            let _ = main_inbox
                .send(FlowgraphMessage::BlockDone { block_id, block })
                .await;
            break;
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
        if let Err(e) = block.work(&mut work_io).await {
            error!(
                "{}: Error in work(). Terminating. ({:?})",
                block.instance_name().unwrap(),
                e
            );
            main_inbox.send(FlowgraphMessage::Terminate).await?;
            return Err(e);
        }
        block.commit();

        futures_lite::future::yield_now().await;
    }

    Ok(())
}
