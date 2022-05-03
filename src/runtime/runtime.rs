#[cfg(not(target_arch = "wasm32"))]
use async_io::block_on;
#[cfg(not(target_arch = "wasm32"))]
use async_task::Task;
use futures::channel::mpsc::{channel, Receiver, Sender};
use futures::future::join_all;
use futures::future::Either;
use futures::prelude::*;
use futures::FutureExt;
#[cfg(target_arch = "wasm32")]
type Task<T> = crate::runtime::scheduler::wasm::TaskHandle<T>;

use crate::anyhow::{Context, Result};
use crate::runtime::config;
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::ctrl_port;
use crate::runtime::scheduler::Scheduler;
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::scheduler::SmolScheduler;
#[cfg(target_arch = "wasm32")]
use crate::runtime::scheduler::WasmScheduler;
use crate::runtime::AsyncMessage;
use crate::runtime::Block;
use crate::runtime::Flowgraph;
use crate::runtime::FlowgraphHandle;
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

    pub fn start(&self, fg: Flowgraph) -> (Task<Result<Flowgraph>>, FlowgraphHandle) {
        let queue_size = config::config().queue_size;
        let (fg_inbox, fg_inbox_rx) = channel::<AsyncMessage>(queue_size);

        let task = self.scheduler.spawn(run_flowgraph(
            fg,
            self.scheduler.clone(),
            fg_inbox.clone(),
            fg_inbox_rx,
        ));
        (task, FlowgraphHandle::new(fg_inbox))
    }

    /// Main method that kicks-off the running of a [Flowgraph].
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run(&self, fg: Flowgraph) -> Result<Flowgraph> {
        let (handle, _) = self.start(fg);
        block_on(handle)
    }

    pub async fn run_async(&self, fg: Flowgraph) -> Result<Flowgraph> {
        let (handle, _) = self.start(fg);
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
    mut main_channel: Sender<AsyncMessage>,
    mut main_rx: Receiver<AsyncMessage>,
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
                .send(AsyncMessage::StreamInputInit {
                    dst_port: *dst_port,
                    reader: writer.add_reader(dst_inbox, *dst_port),
                })
                .await
                .unwrap();
        }

        inboxes[*src]
            .as_mut()
            .unwrap()
            .send(AsyncMessage::StreamOutputInit {
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
            .send(AsyncMessage::MessageOutputConnect {
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
            chan.send(AsyncMessage::Initialize).await.unwrap();
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
            AsyncMessage::Initialized => i -= 1,
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
            if chan.send(AsyncMessage::Notify).await.is_err() {
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
    ctrl_port::start_control_port(inboxes.clone()).await;

    // main loop
    loop {
        if active_blocks == 0 {
            break;
        }

        let m = main_rx.next().await.context("no msg")?;
        match m {
            AsyncMessage::BlockCall {
                block_id,
                port_id,
                data,
            } => {
                inboxes[block_id]
                    .as_mut()
                    .unwrap()
                    .send(AsyncMessage::Call { port_id, data })
                    .await
                    .unwrap();
            }
            AsyncMessage::BlockCallback {
                block_id,
                port_id,
                data,
                tx,
            } => {
                inboxes[block_id]
                    .as_mut()
                    .unwrap()
                    .send(AsyncMessage::Callback { port_id, data, tx })
                    .await
                    .unwrap();
            }
            AsyncMessage::BlockDone { id, block } => {
                *topology.blocks.get_mut(id).unwrap() = Some(block);

                active_blocks -= 1;
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
    mut main_inbox: Sender<AsyncMessage>,
    mut inbox: Receiver<AsyncMessage>,
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
            AsyncMessage::Initialize => {
                block.init().await?;
                main_inbox.send(AsyncMessage::Initialized).await?;
                break;
            }
            AsyncMessage::StreamOutputInit { src_port, writer } => {
                block.stream_output_mut(src_port).init(writer);
            }
            AsyncMessage::StreamInputInit { dst_port, reader } => {
                block.stream_input_mut(dst_port).set_reader(reader);
            }
            AsyncMessage::MessageOutputConnect {
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
                Some(Some(AsyncMessage::Notify)) => {}
                Some(Some(AsyncMessage::StreamInputDone { input_id })) => {
                    block.stream_input_mut(input_id).finish();
                }
                Some(Some(AsyncMessage::StreamOutputDone { .. })) => {
                    work_io.finished = true;
                }
                Some(Some(AsyncMessage::Call { port_id, data })) => {
                    if block.message_input_is_async(port_id) {
                        block.call_async_handler(port_id, data).await?;
                    } else {
                        block.call_sync_handler(port_id, data)?;
                    }
                }
                Some(Some(AsyncMessage::Callback { port_id, data, tx })) => {
                    let res = {
                        if block.message_input_is_async(port_id) {
                            block.call_async_handler(port_id, data).await?
                        } else {
                            block.call_sync_handler(port_id, data)?
                        }
                    };

                    tx.send(res).unwrap();
                }
                Some(Some(AsyncMessage::Terminate)) => work_io.finished = true,
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
            main_inbox
                .send(AsyncMessage::BlockDone {
                    id: block_id,
                    block,
                })
                .await
                .unwrap();
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
        block.work(&mut work_io).await?;

        futures_lite::future::yield_now().await;
    }

    Ok(())
}
