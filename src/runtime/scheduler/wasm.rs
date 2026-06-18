//! WASM Scheduler

use async_task::Runnable;
use concurrent_queue::ConcurrentQueue;
use futures::future::Future;
use gloo_timers::future::TimeoutFuture;
use slab::Slab;
use std::fmt;
use std::panic::RefUnwindSafe;
use std::panic::UnwindSafe;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::*;
use web_sys::Worker;
use web_sys::WorkerOptions;
use web_sys::WorkerType;

use crate::runtime::Error;
use crate::runtime::init;
use crate::runtime::scheduler::NormalDomainSpec;
use crate::runtime::scheduler::NormalRunningDomain;
use crate::runtime::scheduler::RunnableBlock;
use crate::runtime::scheduler::Scheduler;
use crate::runtime::scheduler::StoppedBlock;
use crate::runtime::yield_now;

static WASM_EXECUTORS: once_cell::sync::Lazy<Mutex<Slab<Arc<WasmExecutor>>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(Slab::new()));
static WASM_THREAD_METADATA_RESET: AtomicBool = AtomicBool::new(false);

unsafe extern "C" {
    static __heap_base: u8;
}

pub(crate) const DEFAULT_WORKER_SCRIPT: &str = "./futuresdr-wasm-scheduler-worker.js";

static WASM_WORKER_SCRIPT: once_cell::sync::Lazy<Mutex<String>> =
    once_cell::sync::Lazy::new(|| Mutex::new(DEFAULT_WORKER_SCRIPT.to_string()));

/// Set the default worker script used by the WASM scheduler and local domains.
///
/// Applications that do not serve `./futuresdr-wasm-scheduler-worker.js` should
/// call this before creating [`WasmScheduler`]s or [`LocalDomain`](crate::runtime::LocalDomain)s.
pub fn set_worker_script(worker_script: impl Into<String>) {
    *WASM_WORKER_SCRIPT.lock().unwrap() = worker_script.into();
}

/// Return the default worker script used by the WASM scheduler and local domains.
pub fn worker_script() -> String {
    WASM_WORKER_SCRIPT.lock().unwrap().clone()
}

pub(crate) fn reset_wasm_thread_metadata() {
    if WASM_THREAD_METADATA_RESET.swap(true, Ordering::AcqRel) {
        return;
    }

    let base = core::ptr::addr_of!(__heap_base) as usize;
    if base < 4 {
        return;
    }

    unsafe {
        // wasm-bindgen's thread transform stores its data-init guard at
        // __heap_base - 4, the thread count at __heap_base, and a malloc lock
        // at __heap_base + 4. With current rust/wasm-bindgen output, the main
        // thread's TLS allocation can overwrite these words. Restore them
        // before starting scheduler workers.
        AtomicU32::from_ptr((base - 4) as *mut u32).store(2, Ordering::Release);
        AtomicU32::from_ptr(base as *mut u32).store(1, Ordering::Release);
        AtomicU32::from_ptr((base + 4) as *mut u32).store(0, Ordering::Release);
    }
}

/// Web-worker handle used by the WASM scheduler and WASM local domains.
pub(crate) struct WasmWorker(Worker);

// Web workers are created and owned by the scheduler, but the scheduler itself
// has to satisfy the generic `Scheduler: Send` bound. The actual worker handle
// is only used to post the initial message and to terminate the worker at drop.
unsafe impl Send for WasmWorker {}
unsafe impl Sync for WasmWorker {}

impl WasmWorker {
    pub(crate) fn terminate(self) {
        self.0.terminate();
    }
}

/// WASM scheduler worker entry point.
///
/// Application worker scripts should call this after initializing the generated
/// wasm-bindgen module with the `module` and `memory` values sent by the
/// scheduler.
#[wasm_bindgen]
pub fn futuresdr_wasm_scheduler_worker_entry(executor_id: usize, worker_index: usize) {
    init();
    let executor = WASM_EXECUTORS.lock().unwrap().get(executor_id).cloned();
    if let Some(executor) = executor {
        wasm_bindgen_futures::spawn_local(async move {
            executor.run_until_shutdown(worker_index).await;
        });
    } else {
        error!(
            "WASM scheduler worker got invalid executor id {}",
            executor_id
        );
    }
}

/// Web-worker-backed WASM scheduler.
///
/// The scheduler always runs tasks on web workers, including the default and
/// `WasmScheduler::new(1)` cases. The browser GUI thread is only used by the
/// application code that creates and drives the runtime handles.
#[derive(Clone, Debug)]
pub struct WasmScheduler {
    inner: Arc<WasmSchedulerInner>,
}

struct WasmSchedulerInner {
    executor_id: usize,
    executor: Arc<WasmExecutor>,
    workers: Vec<WasmWorker>,
}

impl fmt::Debug for WasmSchedulerInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WasmSchedulerInner")
            .field("workers", &self.workers.len())
            .finish()
    }
}

impl Drop for WasmSchedulerInner {
    fn drop(&mut self) {
        self.executor.shutdown();

        for worker in self.workers.drain(..) {
            worker.terminate();
        }

        let _ = WASM_EXECUTORS.lock().unwrap().try_remove(self.executor_id);
    }
}

impl WasmScheduler {
    /// Create a scheduler with `n_threads` worker threads.
    ///
    /// A thread count of zero is treated as one. The scheduler expects the
    /// worker script configured with [`set_worker_script`], defaulting to
    /// `./futuresdr-wasm-scheduler-worker.js`.
    pub fn new(n_threads: usize) -> WasmScheduler {
        let worker_script = worker_script();
        let n_threads = n_threads.max(1);

        debug!("WASM scheduler starting {n_threads} web workers with script {worker_script}");
        reset_wasm_thread_metadata();

        let executor = Arc::new(WasmExecutor::new(n_threads));
        let executor_id = WASM_EXECUTORS.lock().unwrap().insert(executor.clone());
        let mut workers = Vec::with_capacity(n_threads);

        for worker_index in 0..n_threads {
            match spawn_worker(&worker_script, executor_id, worker_index) {
                Ok(worker) => workers.push(worker),
                Err(e) => {
                    for worker in workers.drain(..) {
                        worker.terminate();
                    }
                    let _ = WASM_EXECUTORS.lock().unwrap().try_remove(executor_id);
                    panic!(
                        "failed to spawn WASM scheduler web worker from {worker_script:?}: {e:?}. \
                         Serve a worker script that dispatches FutureSDR scheduler/local-domain \
                         init messages, or configure it with \
                         futuresdr::runtime::scheduler::wasm::set_worker_script(path)."
                    );
                }
            }
        }

        debug!("WASM scheduler started {} web workers", workers.len());
        WasmScheduler {
            inner: Arc::new(WasmSchedulerInner {
                executor_id,
                executor,
                workers,
            }),
        }
    }

    /// Return the number of worker threads used by this scheduler.
    pub fn n_threads(&self) -> usize {
        self.inner.workers.len()
    }
}

impl Scheduler for WasmScheduler {
    fn start_normal_domain(&self, spec: NormalDomainSpec) -> Result<NormalRunningDomain, Error> {
        let mut spec = spec;
        let block_ids = spec.blocks().collect::<Vec<_>>();
        let n_blocks = block_ids.len();
        let n_threads = self.inner.workers.len();
        let mut blocks = Vec::with_capacity(n_blocks);

        for (block_index, block_id) in block_ids.into_iter().enumerate() {
            let block = spec.take_block(block_id)?;
            let stop = block.stop_handle();
            let worker_index = block_index * n_threads / n_blocks;
            blocks.push((
                spawn_wasm_block(&self.inner.executor, block, worker_index),
                stop,
            ));
        }

        Ok(NormalRunningDomain::new(blocks))
    }

    fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        Task::new(self.inner.executor.spawn(future))
    }
}

impl Default for WasmScheduler {
    fn default() -> Self {
        Self::new(1)
    }
}

/// Main-thread WASM scheduler.
///
/// This scheduler runs the runtime and all normal blocks on the browser main
/// thread instead of FutureSDR scheduler web workers. It is useful for blocks
/// that must create or own browser-main-thread APIs such as Web Audio through
/// CPAL's WebAudio backend. CPU-heavy flowgraphs should prefer
/// [`WasmScheduler`] to avoid blocking the UI thread.
#[derive(Clone, Copy, Debug, Default)]
pub struct WasmMainScheduler;

impl WasmMainScheduler {
    /// Create a main-thread WASM scheduler.
    pub fn new() -> Self {
        Self
    }
}

impl Scheduler for WasmMainScheduler {
    fn start_normal_domain(&self, spec: NormalDomainSpec) -> Result<NormalRunningDomain, Error> {
        let mut spec = spec;
        let block_ids = spec.blocks().collect::<Vec<_>>();
        let mut blocks = Vec::with_capacity(block_ids.len());
        for block_id in block_ids {
            let block = spec.take_block(block_id)?;
            let stop = block.stop_handle();
            blocks.push((spawn_wasm_main_block(block), stop));
        }
        Ok(NormalRunningDomain::new(blocks))
    }

    fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        Task::new(spawn_main(future))
    }
}

fn spawn_worker(
    worker_script: &str,
    executor_id: usize,
    worker_index: usize,
) -> Result<WasmWorker, JsValue> {
    let options = WorkerOptions::new();
    options.set_type(WorkerType::Module);
    let worker = Worker::new_with_options(worker_script, &options)?;
    let init = js_sys::Object::new();

    js_sys::Reflect::set(
        &init,
        &JsValue::from_str("type"),
        &JsValue::from_str("futuresdr-wasm-scheduler-init"),
    )?;
    js_sys::Reflect::set(&init, &JsValue::from_str("module"), &wasm_bindgen::module())?;
    js_sys::Reflect::set(&init, &JsValue::from_str("memory"), &wasm_bindgen::memory())?;
    js_sys::Reflect::set(
        &init,
        &JsValue::from_str("executor_id"),
        &JsValue::from_f64(executor_id as f64),
    )?;
    js_sys::Reflect::set(
        &init,
        &JsValue::from_str("worker_index"),
        &JsValue::from_f64(worker_index as f64),
    )?;
    if let Err(e) = worker.post_message(&init) {
        worker.terminate();
        return Err(e);
    }

    Ok(WasmWorker(worker))
}

pub(crate) fn spawn_local_domain_worker(
    worker_script: &str,
    domain_id: usize,
) -> Result<WasmWorker, JsValue> {
    reset_wasm_thread_metadata();

    let options = WorkerOptions::new();
    options.set_type(WorkerType::Module);
    let worker = Worker::new_with_options(worker_script, &options)?;
    let init = js_sys::Object::new();

    js_sys::Reflect::set(
        &init,
        &JsValue::from_str("type"),
        &JsValue::from_str("futuresdr-wasm-local-domain-init"),
    )?;
    js_sys::Reflect::set(&init, &JsValue::from_str("module"), &wasm_bindgen::module())?;
    js_sys::Reflect::set(&init, &JsValue::from_str("memory"), &wasm_bindgen::memory())?;
    js_sys::Reflect::set(
        &init,
        &JsValue::from_str("domain_id"),
        &JsValue::from_f64(domain_id as f64),
    )?;
    if let Err(e) = worker.post_message(&init) {
        worker.terminate();
        return Err(e);
    }

    Ok(WasmWorker(worker))
}

fn spawn_wasm_block(
    executor: &WasmExecutor,
    block: RunnableBlock,
    queue_index: usize,
) -> Task<StoppedBlock> {
    Task::new(executor.spawn_executor(block.run(), queue_index))
}

fn spawn_wasm_main_block(block: RunnableBlock) -> Task<StoppedBlock> {
    Task::new(spawn_main(block.run()))
}

fn spawn_main<T: Send + 'static>(
    future: impl Future<Output = T> + Send + 'static,
) -> async_task::Task<T> {
    let schedule = |runnable: Runnable| {
        wasm_bindgen_futures::spawn_local(async move {
            runnable.run();
        });
    };
    let (runnable, task) = async_task::spawn(future, schedule);
    runnable.schedule();
    task
}

/// WASM async task.
pub struct Task<T>(async_task::Task<T>);

impl<T> Task<T> {
    fn new(task: async_task::Task<T>) -> Self {
        Task(task)
    }

    /// Detach the task so it continues running when the task handle is dropped.
    pub fn detach(self) {
        self.0.detach();
    }
}

impl<T> std::future::Future for Task<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.get_mut().0).poll(cx)
    }
}

/// A small async executor with per-worker local queues.
struct WasmExecutor {
    state: once_cell::sync::OnceCell<Arc<State>>,
    worker_count: usize,
}

const LOCAL_QUEUE_CAPACITY: usize = 512;

impl UnwindSafe for WasmExecutor {}
impl RefUnwindSafe for WasmExecutor {}

impl WasmExecutor {
    const fn new(worker_count: usize) -> WasmExecutor {
        WasmExecutor {
            state: once_cell::sync::OnceCell::new(),
            worker_count,
        }
    }

    fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> async_task::Task<T> {
        let mut active = self.state().active.lock().unwrap();

        let entry = active.vacant_entry();
        let key = entry.key();
        let state = self.state().clone();
        let future = async move {
            let _guard = CallOnDrop(move || drop(state.active.lock().unwrap().try_remove(key)));
            future.await
        };

        let (runnable, task) = unsafe { async_task::spawn_unchecked(future, self.schedule()) };
        entry.insert(runnable.waker());

        runnable.schedule();
        task
    }

    fn spawn_executor<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
        executor: usize,
    ) -> async_task::Task<T> {
        let mut active = self.state().active.lock().unwrap();

        let entry = active.vacant_entry();
        let key = entry.key();
        let state = self.state().clone();
        let future = async move {
            let _guard = CallOnDrop(move || drop(state.active.lock().unwrap().try_remove(key)));
            future.await
        };

        let local = self
            .state()
            .local_queues
            .get(executor)
            .cloned()
            .expect("executor queue not initialized");

        let (runnable, task) =
            unsafe { async_task::spawn_unchecked(future, self.schedule_executor(local)) };
        entry.insert(runnable.waker());

        runnable.schedule();
        task
    }

    async fn run_until_shutdown(&self, worker_index: usize) {
        let state = self.state().clone();
        let mut runner = Runner::new(self.state(), worker_index);

        while !state.shutdown.load(Ordering::Acquire) {
            let mut ran = false;
            for _ in 0..200 {
                let Some(runnable) = runner.try_runnable() else {
                    break;
                };
                runnable.run();
                ran = true;
            }

            if ran {
                yield_now().await;
            } else {
                TimeoutFuture::new(1).await;
            }
        }
    }

    fn shutdown(&self) {
        self.state().shutdown.store(true, Ordering::Release);
    }

    fn schedule(&self) -> impl Fn(Runnable) + Send + Sync + 'static {
        let state = self.state().clone();

        move |runnable| {
            state.queue.push(runnable).unwrap();
        }
    }

    fn schedule_executor(
        &self,
        local: Arc<ConcurrentQueue<Runnable>>,
    ) -> impl Fn(Runnable) + Send + Sync + 'static {
        let state = self.state().clone();

        move |runnable| {
            if let Err(err) = local.push(runnable) {
                state.queue.push(err.into_inner()).unwrap();
            }
        }
    }

    fn state(&self) -> &Arc<State> {
        self.state
            .get_or_init(|| Arc::new(State::new(self.worker_count)))
    }
}

impl Drop for WasmExecutor {
    fn drop(&mut self) {
        debug!("dropping WASM executor");
        if let Some(state) = self.state.get() {
            let active = state.active.lock().unwrap();

            for (_, w) in active.iter() {
                w.wake_by_ref();
            }

            drop(active);

            while state.queue.pop().is_ok() {}

            for q in state.local_queues.iter() {
                while q.pop().is_ok() {}
            }
        }
    }
}

struct State {
    queue: ConcurrentQueue<Runnable>,
    shutdown: AtomicBool,
    local_queues: Vec<Arc<ConcurrentQueue<Runnable>>>,
    active: Mutex<Slab<Waker>>,
}

impl State {
    fn new(worker_count: usize) -> State {
        let local_queues: Vec<_> = (0..worker_count)
            .map(|_| Arc::new(ConcurrentQueue::bounded(LOCAL_QUEUE_CAPACITY)))
            .collect();

        State {
            queue: ConcurrentQueue::unbounded(),
            shutdown: AtomicBool::new(false),
            local_queues,
            active: Mutex::new(Slab::new()),
        }
    }
}

struct Runner<'a> {
    state: &'a State,
    local: Arc<ConcurrentQueue<Runnable>>,
}

impl Runner<'_> {
    fn new(state: &State, worker_index: usize) -> Runner<'_> {
        let local = state
            .local_queues
            .get(worker_index)
            .cloned()
            .expect("worker local queue not initialized");

        Runner { state, local }
    }

    fn try_runnable(&mut self) -> Option<Runnable> {
        if let Ok(r) = self.local.pop() {
            return Some(r);
        }

        if let Ok(r) = self.state.queue.pop() {
            return Some(r);
        }

        None
    }
}

struct CallOnDrop<F: Fn()>(F);

impl<F: Fn()> Drop for CallOnDrop<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}
