use std::thread;

use crate::runtime::Error;
use crate::runtime::block_inbox::LocalDomainKey;
use crate::runtime::block_on;
use crate::runtime::channel::mpsc;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::channel::oneshot;
use crate::runtime::config;
use crate::runtime::local_domain_common::IdleDomainAction;
pub(crate) use crate::runtime::local_domain_common::LocalDomainInbox;
use crate::runtime::local_domain_common::LocalDomainMessage;
use crate::runtime::local_domain_common::LocalDomainRuntimeBase;
use crate::runtime::local_domain_common::LocalDomainState;
use crate::runtime::local_domain_common::finish_local_run_result;
use crate::runtime::local_domain_common::handle_idle_domain_message;
use crate::runtime::scheduler::LocalScheduler;
use crate::runtime::scheduler::dev::LocalDomainRunSpec;

pub(crate) type LocalDomainRuntime = LocalDomainRuntimeBase<LocalDomainController>;

impl LocalDomainRuntime {
    pub(crate) fn new<LS: LocalScheduler>() -> Result<Self, Error> {
        Self::new_pinned::<LS>(None)
    }

    pub(crate) fn new_pinned<LS: LocalScheduler>(cpuid: Option<usize>) -> Result<Self, Error> {
        let controller = LocalDomainController::new_pinned::<LS>(cpuid)?;
        let inbox = LocalDomainInbox::new(controller.tx.clone(), controller.key);
        Ok(Self::from_controller(controller, inbox))
    }
}

pub(crate) struct LocalDomainController {
    tx: Sender<LocalDomainMessage>,
    key: LocalDomainKey,
    terminate_tx: Option<oneshot::Sender<()>>,
    join: Option<thread::JoinHandle<()>>,
}

impl LocalDomainController {
    pub(crate) fn new_pinned<LS: LocalScheduler>(cpuid: Option<usize>) -> Result<Self, Error> {
        let (tx, rx) = mpsc::channel(config::config().queue_size);
        let key = LocalDomainKey::new();
        let (terminate_tx, terminate_rx) = oneshot::channel();
        let thread_name = cpuid
            .map(|cpuid| format!("futuresdr-local-{cpuid}"))
            .unwrap_or_else(|| "futuresdr-local".to_string());
        let join = thread::Builder::new()
            .stack_size(config::config().stack_size)
            .name(thread_name)
            .spawn(move || {
                if let Some(cpuid) = cpuid {
                    let core_id = core_affinity::CoreId { id: cpuid };
                    debug!("starting local domain thread on core id {}", cpuid);
                    if !core_affinity::set_for_current(core_id) {
                        warn!("failed to pin local domain thread to core id {}", cpuid);
                    }
                }
                block_on(run_domain_thread::<LS>(rx, terminate_rx, key))
            })
            .map_err(|e| {
                Error::RuntimeError(format!("failed to spawn local domain thread: {e}"))
            })?;

        Ok(Self {
            tx,
            key,
            terminate_tx: Some(terminate_tx),
            join: Some(join),
        })
    }
}

impl Drop for LocalDomainController {
    fn drop(&mut self) {
        if let Some(terminate_tx) = self.terminate_tx.take() {
            let _ = terminate_tx.send(());
        }
        let _ = self.tx.try_send(LocalDomainMessage::Terminate);
        let _ = self.tx.close();
        if let Some(join) = self.join.take()
            && join.join().is_err()
        {
            debug!("local domain thread panicked during shutdown");
        }
    }
}

async fn run_domain_thread<LS: LocalScheduler>(
    mut rx: mpsc::Receiver<LocalDomainMessage>,
    mut terminate_rx: oneshot::Receiver<()>,
    key: LocalDomainKey,
) {
    let mut state = LocalDomainState::new();
    let scheduler = LS::default();

    while let Some(message) = rx.recv().await {
        match handle_idle_domain_message(message, &mut state, &scheduler).await {
            IdleDomainAction::Continue => {}
            IdleDomainAction::Run {
                domain_id,
                slots,
                topology,
                main_channel,
                reply,
            } => {
                let mut running_state = match state.start_run(&slots) {
                    Ok(state) => state,
                    Err(e) => {
                        let _ = reply.send(Err(e));
                        continue;
                    }
                };
                let spec = LocalDomainRunSpec {
                    domain_id,
                    slots,
                    topology,
                    state: &mut running_state,
                    main_channel,
                    shutdown: &mut terminate_rx,
                    domain_rx: &mut rx,
                    key,
                    external_inboxes: Vec::new(),
                };
                let run_result = scheduler.run_local_domain(spec).await;
                let finish_result = state.finish_run(running_state);
                let _ = reply.send(finish_local_run_result(run_result, finish_result));
            }
            IdleDomainAction::Terminate => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::runtime::BlockId;
    use crate::runtime::BlockMessage;
    use crate::runtime::FlowgraphMessage;
    use crate::runtime::PortIndex;
    use crate::runtime::PortName;
    use crate::runtime::block::BlockObject;
    use crate::runtime::block::LocalBlock;
    use crate::runtime::block_inbox::BlockEndpoint;
    use crate::runtime::block_inbox::BlockInbox;
    use crate::runtime::block_inbox::BlockInboxReader;
    use crate::runtime::block_inbox::LocalBlockInbox;
    use crate::runtime::block_inbox::LocalBlockInboxReader;
    use crate::runtime::buffer::DynBufferReader;
    use crate::runtime::buffer::DynBufferWriter;
    use crate::runtime::local_domain_common::build_local_block;
    use crate::runtime::scheduler::BasicLocalScheduler;
    use crate::runtime::scheduler::dev::DomainTopology;

    struct WaitForTerminate {
        id: BlockId,
        inbox: BlockEndpoint,
        local_inbox: LocalBlockInbox,
        local_inbox_rx: LocalBlockInboxReader,
        started: Option<oneshot::Sender<()>>,
    }

    impl BlockObject for WaitForTerminate {
        fn inbox(&self) -> BlockEndpoint {
            self.inbox.clone()
        }

        fn id(&self) -> BlockId {
            self.id
        }

        fn stream_input_at(
            &mut self,
            _index: PortIndex,
        ) -> Option<(PortName, &mut dyn DynBufferReader)> {
            None
        }

        fn stream_output_at(
            &mut self,
            _index: PortIndex,
        ) -> Option<(PortName, &mut dyn DynBufferWriter)> {
            None
        }

        fn message_inputs(&self) -> &'static [&'static str] {
            &[]
        }

        fn message_outputs(&self) -> &'static [&'static str] {
            &[]
        }

        fn connect_message(
            &mut self,
            _src_port: PortIndex,
            _dst: BlockEndpoint,
            _dst_port: PortIndex,
        ) -> Result<(), Error> {
            Ok(())
        }

        fn type_name(&self) -> &str {
            "WaitForTerminate"
        }

        fn instance_name(&self) -> Option<&str> {
            None
        }

        fn is_blocking(&self) -> bool {
            false
        }
    }

    #[async_trait::async_trait(?Send)]
    impl LocalBlock for WaitForTerminate {
        fn local_inbox(&self) -> LocalBlockInbox {
            self.local_inbox.clone()
        }

        fn take_external_inbox_reader(&mut self) -> Option<BlockInboxReader> {
            None
        }

        async fn run(&mut self, main_inbox: Sender<FlowgraphMessage>) {
            if let Some(started) = self.started.take() {
                let _ = started.send(());
            }

            while let Some(message) = self.local_inbox_rx.recv().await {
                if matches!(message, BlockMessage::Terminate) {
                    break;
                }
            }

            let _ = main_inbox
                .send(FlowgraphMessage::BlockDone { block_id: self.id })
                .await;
        }
    }

    #[test]
    fn controller_drop_terminates_running_local_blocks() -> Result<(), Error> {
        let controller = LocalDomainController::new_pinned::<BasicLocalScheduler>(None)?;
        let (started_tx, started_rx) = oneshot::channel();
        block_on(build_local_block(
            &controller.tx,
            Box::new(|local_id| {
                let (inbox, _inbox_rx) = BlockInbox::pair(4);
                let (local_inbox, local_inbox_rx) = LocalBlockInboxReader::pair();
                Box::new(WaitForTerminate {
                    id: BlockId(local_id),
                    inbox: inbox.into(),
                    local_inbox,
                    local_inbox_rx,
                    started: Some(started_tx),
                })
            }),
        ))?;
        let (main_tx, _main_rx) = mpsc::channel(4);
        let run = LocalDomainInbox::new(controller.tx.clone(), controller.key).start_run(
            0,
            vec![(BlockId(0), 0)],
            DomainTopology::new(vec![BlockId(0)], vec![], vec![]),
            main_tx,
        )?;

        block_on(started_rx)
            .map_err(|_| Error::RuntimeError("local block did not start".to_string()))?;
        drop(controller);

        block_on(run).map_err(|_| Error::RuntimeError("local domain task canceled".to_string()))?
    }
}
