use futures::Future;
use slab::Slab;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use wasm_bindgen::prelude::*;

use crate::runtime::Error;
use crate::runtime::block_inbox::LocalDomainKey;
use crate::runtime::channel::mpsc;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::local_domain_common::IdleDomainAction;
use crate::runtime::local_domain_common::LocalDomainControllerAccess;
pub(crate) use crate::runtime::local_domain_common::LocalDomainInbox;
use crate::runtime::local_domain_common::LocalDomainMessage;
use crate::runtime::local_domain_common::LocalDomainRuntimeBase;
use crate::runtime::local_domain_common::LocalDomainState;
use crate::runtime::local_domain_common::handle_idle_domain_message;
use crate::runtime::scheduler::LocalDomainRunSpec;
use crate::runtime::scheduler::LocalScheduler;
use crate::runtime::scheduler::wasm::WasmWorker;
use crate::runtime::scheduler::wasm::spawn_local_domain_worker;

pub(crate) type LocalDomainRuntime = LocalDomainRuntimeBase<LocalDomainController>;

impl LocalDomainRuntime {
    pub(crate) fn new<LS: LocalScheduler>() -> Result<Self, Error> {
        Ok(Self::from_controller(LocalDomainController::new::<LS>()?))
    }
}

pub(crate) struct LocalDomainController {
    tx: Sender<LocalDomainMessage>,
    key: LocalDomainKey,
    terminate: Arc<AtomicBool>,
    worker: Option<WasmWorker>,
    domain_id: Option<usize>,
}

impl LocalDomainController {
    pub(crate) fn new<LS: LocalScheduler>() -> Result<Self, Error> {
        let (tx, rx) = mpsc::channel(crate::runtime::config::config().queue_size);
        let key = LocalDomainKey::new();
        let terminate = Arc::new(AtomicBool::new(false));
        let init = WasmLocalDomainInit {
            rx,
            key,
            terminate: terminate.clone(),
            runner: run_domain_worker_boxed::<LS>,
        };
        let domain_id = WASM_LOCAL_DOMAINS.lock().unwrap().insert(init);
        let worker_script = default_worker_script();
        let worker = spawn_local_domain_worker(&worker_script, domain_id).map_err(|e| {
            let _ = WASM_LOCAL_DOMAINS.lock().unwrap().try_remove(domain_id);
            Error::RuntimeError(format!(
                "failed to spawn WASM local-domain worker from {worker_script:?}: {e:?}. \
                 Serve a worker script that dispatches FutureSDR scheduler/local-domain init \
                 messages, or configure it with \
                 futuresdr::runtime::scheduler::wasm::set_worker_script(path)."
            ))
        })?;

        Ok(Self {
            tx,
            key,
            terminate,
            worker: Some(worker),
            domain_id: Some(domain_id),
        })
    }
}

impl LocalDomainControllerAccess for LocalDomainController {
    fn tx(&self) -> &Sender<LocalDomainMessage> {
        &self.tx
    }

    fn key(&self) -> LocalDomainKey {
        self.key
    }
}

impl Drop for LocalDomainController {
    fn drop(&mut self) {
        self.terminate.store(true, Ordering::Release);
        let _ = self.tx.try_send(LocalDomainMessage::Terminate);
        if let Some(id) = self.domain_id.take() {
            let _ = WASM_LOCAL_DOMAINS.lock().unwrap().try_remove(id);
        }
        if let Some(worker) = self.worker.take() {
            worker.terminate();
        }
    }
}

static WASM_LOCAL_DOMAINS: once_cell::sync::Lazy<Mutex<Slab<WasmLocalDomainInit>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(Slab::new()));

struct WasmLocalDomainInit {
    rx: mpsc::Receiver<LocalDomainMessage>,
    key: LocalDomainKey,
    terminate: Arc<AtomicBool>,
    runner: fn(WasmLocalDomainInit) -> Pin<Box<dyn Future<Output = ()>>>,
}

fn default_worker_script() -> String {
    crate::runtime::scheduler::wasm::worker_script()
}

/// WASM local-domain worker entry point.
///
/// Application worker scripts should call this after initializing the generated
/// wasm-bindgen module with the `module` and `memory` values sent by the local
/// domain runtime.
#[wasm_bindgen]
pub fn futuresdr_wasm_local_domain_worker_entry(domain_id: usize) {
    crate::runtime::init();
    let init = WASM_LOCAL_DOMAINS.lock().unwrap().try_remove(domain_id);
    if let Some(init) = init {
        wasm_bindgen_futures::spawn_local((init.runner)(init));
    } else {
        error!(
            "WASM local-domain worker got invalid domain id {}",
            domain_id
        );
    }
}

fn run_domain_worker_boxed<LS: LocalScheduler>(
    init: WasmLocalDomainInit,
) -> Pin<Box<dyn Future<Output = ()>>> {
    Box::pin(run_domain_worker::<LS>(init))
}

async fn run_domain_worker<LS: LocalScheduler>(init: WasmLocalDomainInit) {
    let WasmLocalDomainInit {
        mut rx,
        key,
        terminate,
        ..
    } = init;
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
                let mut shutdown = std::future::poll_fn({
                    let terminate = terminate.clone();
                    move |_| {
                        if terminate.load(Ordering::Acquire) {
                            std::task::Poll::Ready(())
                        } else {
                            std::task::Poll::Pending
                        }
                    }
                });
                let spec = LocalDomainRunSpec {
                    domain_id,
                    slots,
                    topology,
                    state: &mut state,
                    main_channel,
                    shutdown: &mut shutdown,
                    domain_rx: &mut rx,
                    key,
                    external_inboxes: Vec::new(),
                };
                let result = scheduler.run_local_domain(spec).await;
                let _ = reply.send(result);
                if terminate.load(Ordering::Acquire) {
                    break;
                }
            }
            IdleDomainAction::Terminate => break,
        }
    }
}
