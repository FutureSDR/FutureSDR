use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;
use futures::{channel, SinkExt, StreamExt};
use gloo_net::websocket::futures::WebSocket;
use gloo_net::websocket::Message;
use gloo_net::websocket::WebSocketError::{ConnectionClose, ConnectionError, MessageSendError};
use std::marker::PhantomData;
use std::mem::size_of;
use std::sync::Arc;
use std::sync::RwLock;
use wasm_bindgen_futures::spawn_local;

pub struct WasmWsSink<T> {
    data_sender: channel::mpsc::Sender<Vec<u8>>,
    data_storage: Vec<u8>,
    iterations_per_send: usize,
    ws_error: Arc<RwLock<bool>>,
    _p: PhantomData<T>,
}

impl<T: Send + Sync + 'static> WasmWsSink<T> {
    pub fn new(url: String, iterations_per_send: usize) -> Block {
        let (sender, mut receiver) = channel::mpsc::channel::<Vec<u8>>(1);

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

        Block::new(
            BlockMetaBuilder::new("WasmWsSink").build(),
            StreamIoBuilder::new()
                .add_input("in", size_of::<T>())
                .build(),
            MessageIoBuilder::<Self>::new().build(),
            WasmWsSink {
                data_sender: sender,
                data_storage: Vec::new(),
                iterations_per_send,
                ws_error,
                _p: PhantomData,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: Send + Sync + 'static> Kernel for WasmWsSink<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        // Check whether an error has occurred in the websocket task before this call to `work`.
        if *self.ws_error.read().expect("Lock is poisoned") {
            anyhow::bail!("WebSocket Error");
        }
        let i = sio.input(0).slice::<u8>();
        debug_assert_eq!(i.len() % size_of::<T>(), 0);

        // The frontend requires 2048 f32 values per receive. We only produce multiple of 2048 to
        // satisfy this constraint.
        let items_to_process_per_run = 2048;

        if i.is_empty() {
            return Ok(());
        }

        if sio.input(0).finished() {
            io.finished = true;
        }

        let mut v = Vec::new();
        let item_size = size_of::<T>();
        let items = i.len() / item_size;
        // Do not process data until at least `items_to_process_per_run` are in the input buffer.
        if items_to_process_per_run <= items {
            v.extend_from_slice(&i[0..(items_to_process_per_run * (item_size / size_of::<u8>()))]);
            sio.input(0).consume(items_to_process_per_run);
        }

        // If there are at least `items_to_process_per_run` items still remaining in the input buffer
        // set `call_again`.
        if (items - items_to_process_per_run) >= items_to_process_per_run {
            io.call_again = true;
        }
        if !v.is_empty() {
            self.data_storage.append(&mut v);
            // Send data only if the `iterations_per_send` is reached.
            if self.data_storage.len()
                >= items_to_process_per_run
                    * self.iterations_per_send
                    * (item_size / size_of::<u8>())
            {
                let mut movable_vector = Vec::with_capacity(
                    items_to_process_per_run
                        * self.iterations_per_send
                        * (item_size / size_of::<u8>()),
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

        Ok(())
    }
}
