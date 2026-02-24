use any_spawner::Executor;
use anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::MessagePipe;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::wasm::HackRf;
use futuresdr::futures::SinkExt;
use futuresdr::futures::StreamExt;
use futuresdr::futures::channel::mpsc::Receiver;
use futuresdr::prelude::*;
use gloo_worker::HandlerId;
use gloo_worker::WorkerScope;
use leptos::task::spawn_local;

use crate::ClockRecoveryMm;
use crate::Decoder;
use crate::Mac;

#[derive(serde::Serialize, serde::Deserialize)]
pub enum WorkerMessage {
    Start,
    Freq(u64),
}

enum Handle {
    None,
    Receiver(Receiver<FlowgraphHandle>),
    Flowgraph(FlowgraphHandle),
}

pub struct Worker {
    started: bool,
    handle: Handle,
}

impl gloo_worker::Worker for Worker {
    type Input = WorkerMessage;
    type Message = ();
    type Output = Frame;

    fn create(_scope: &WorkerScope<Self>) -> Self {
        Executor::init_wasm_bindgen().unwrap();
        Self {
            started: false,
            handle: Handle::None,
        }
    }

    fn update(&mut self, _scope: &WorkerScope<Self>, _msg: Self::Message) {}

    fn received(&mut self, scope: &WorkerScope<Self>, msg: Self::Input, id: HandlerId) {
        match msg {
            WorkerMessage::Start => {
                if self.started {
                    return;
                }
                self.started = true;
                let (mut set_handler, get_handle) = mpsc::channel::<FlowgraphHandle>(1);
                self.handle = Handle::Receiver(get_handle);
                let scope = scope.clone();
                spawn_local(async move {
                    async move {
                        let mut fg = Flowgraph::new();

                        let src = HackRf::new();

                        let mut last: Complex32 = Complex32::new(0.0, 0.0);
                        let mut iir: f32 = 0.0;
                        let alpha = 0.00016;
                        let avg = Apply::<_, _, _>::new(move |i: &Complex32| -> f32 {
                            let phase = (last.conj() * i).arg();
                            last = *i;
                            iir = (1.0 - alpha) * iir + alpha * phase;
                            phase - iir
                        });

                        let omega = 2.0;
                        let gain_omega = 0.000225;
                        let mu = 0.5;
                        let gain_mu = 0.03;
                        let omega_relative_limit = 0.0002;
                        let mm: ClockRecoveryMm = ClockRecoveryMm::new(
                            omega,
                            gain_omega,
                            mu,
                            gain_mu,
                            omega_relative_limit,
                        );

                        let decoder = Decoder::new(6);
                        let mac: Mac = Mac::new();
                        let snk = NullSink::<u8>::new();

                        let (tx_frame, mut rx_frame) = mpsc::channel::<Pmt>(100);
                        let message_pipe = MessagePipe::new(tx_frame);

                        connect!(fg, src > avg > mm > decoder;
                                     mac > snk;
                                     decoder | rx.mac;
                                     mac.rxed | message_pipe);

                        let rt = Runtime::new();

                        let (_task, handle) = rt.start(fg).await?;
                        set_handler.send(handle).await.unwrap();

                        futuresdr::tracing::info!("waiting for frames");
                        while let Some(x) = rx_frame.next().await {
                            info!("rxed {:?}", x);
                            match x {
                                Pmt::Blob(data) => scope.respond(id, Frame::new(data)),
                                e => futuresdr::tracing::info!("rx_frame broke {:?}", e),
                            }
                        }
                        Result::<()>::Ok(())
                    }
                    .await
                    .unwrap()
                });
            }
            WorkerMessage::Freq(f) => match &mut self.handle {
                Handle::None => {}
                Handle::Receiver(r) => {
                    if let Ok(mut h) = r.try_recv() {
                        self.handle = Handle::Flowgraph(h.clone());
                        spawn_local(async move {
                            h.call(BlockId(6), "freq", Pmt::U64(f)).await.unwrap();
                        });
                    }
                }
                Handle::Flowgraph(h) => {
                    let mut h = h.clone();
                    spawn_local(async move {
                        h.call(BlockId(6), "freq", Pmt::U64(f)).await.unwrap();
                    });
                }
            },
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Frame {
    dst_addr: String,
    dst_pan: String,
    crc: String,
    payload: String,
}

impl Frame {
    fn new(data: Vec<u8>) -> Self {
        let dst_pan = format!(
            "{:#06x}",
            u16::from_le_bytes(data[3..5].try_into().unwrap())
        );
        let dst_addr = format!(
            "{:#06x}",
            u16::from_le_bytes(data[5..7].try_into().unwrap())
        );
        let payload = String::from_utf8_lossy(&data[7..data.len() - 2]).to_string();
        let crc = format!(
            "{:#06x}",
            u16::from_le_bytes(data[data.len() - 2..data.len()].try_into().unwrap())
        );

        Frame {
            dst_addr,
            dst_pan,
            crc,
            payload,
        }
    }
}
