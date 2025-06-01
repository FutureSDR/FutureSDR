use futures::SinkExt;
use futures::StreamExt;
use gloo_net::websocket::Message;
use gloo_net::websocket::WebSocketError::ConnectionClose;
use gloo_net::websocket::WebSocketError::ConnectionError;
use gloo_net::websocket::WebSocketError::MessageSendError;
use gloo_net::websocket::futures::WebSocket;
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::RwLock;
use wasm_bindgen_futures::spawn_local;

use crate::prelude::*;

/// WASM Websocket Sink
#[derive(Block)]
pub struct WasmWsSink<T>
where
    T: CpuSample,
{
    #[input]
    input: slab::Reader<T>,
    data_sender: mpsc::Sender<Vec<u8>>,
    data_storage: Vec<u8>,
    iterations_per_send: usize,
    ws_error: Arc<RwLock<bool>>,
    _p: PhantomData<T>,
}

impl<T> WasmWsSink<T>
where
    T: CpuSample,
{
    /// Create WASM Websocket Sink block
    pub fn new(url: String, iterations_per_send: usize) -> Self {
        let (sender, mut receiver) = mpsc::channel::<Vec<u8>>(1);

        let ws_error = Arc::new(RwLock::new(false));
        let ws_error_clone = ws_error.clone();
        // Spawn the websocket sender task. If an error occurs, the `ws_error_clone` variable is set
        // to true. This variable is checked in the processing and will return an error to any
        // following processing attempt (call to `work`).
        spawn_local(async move {
            if let Ok(mut conn) = WebSocket::open(&url) {
                while let Some(v) = receiver.next().await {
                    if let Err(error) = conn.send(Message::Bytes(v)).await {
                        // On error, set `ws_error` to true.
                        match error {
                            ConnectionError => {
                                *ws_error_clone.write().expect("Lock is poisoned") = true;
                            }
                            ConnectionClose(_close_event) => {
                                *ws_error_clone.write().expect("Lock is poisoned") = true;
                            }
                            MessageSendError(_) => {
                                *ws_error_clone.write().expect("Lock is poisoned") = true;
                            }
                            _ => {
                                // Error enum is marked non-exhaustive
                                panic!("New gloo_net websocket error");
                            }
                        }
                    }
                }
            } else {
                *ws_error_clone.write().expect("Lock is poisoned") = true;
            }
        });

        WasmWsSink {
            input: slab::Reader::default(),
            data_sender: sender,
            data_storage: Vec::new(),
            iterations_per_send,
            ws_error,
            _p: PhantomData,
        }
    }
}

#[doc(hidden)]
impl<T> Kernel for WasmWsSink<T>
where
    T: CpuSample,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        // Check whether an error has occurred in the websocket task before this call to `work`.
        if *self.ws_error.read().expect("Lock is poisoned") {
            anyhow::bail!("WebSocket Error");
        }

        let i = self.input.slice();
        let i_len = i.len();
        if i.is_empty() {
            return Ok(());
        }

        // The frontend requires 2048 f32 values per receive. We only produce multiple of 2048 to
        // satisfy this constraint.
        let items_to_process_per_run = 2048;

        // Do not process data until at least `items_to_process_per_run` are in the input buffer.
        if items_to_process_per_run <= i_len {
            let len_bytes = items_to_process_per_run * std::mem::size_of::<T>();
            let s = unsafe { std::slice::from_raw_parts(i.as_ptr() as *const u8, len_bytes) };
            self.data_storage.extend_from_slice(s);

            self.input.consume(items_to_process_per_run);

            // Send data only if the `iterations_per_send` is reached.
            if self.data_storage.len()
                >= items_to_process_per_run * self.iterations_per_send * std::mem::size_of::<T>()
            {
                let mut movable_vector = Vec::with_capacity(
                    items_to_process_per_run * self.iterations_per_send * std::mem::size_of::<T>(),
                );
                std::mem::swap(&mut self.data_storage, &mut movable_vector);
                // If send fails, we cannot gracefully recover so we panic.
                // https://docs.rs/futures-channel/latest/futures_channel/mpsc/struct.Sender.html#method.poll_ready
                self.data_sender
                    .send(movable_vector)
                    .await
                    .expect("Receiver has been dropped, we cannot gracefully recover");
            }
        }

        // If there are at least `items_to_process_per_run` items still remaining in the input buffer
        // set `call_again`.
        if (i_len - items_to_process_per_run) >= items_to_process_per_run {
            io.call_again = true;
        }

        if self.input.finished() {
            io.finished = true;
        }

        Ok(())
    }
}
