use any_spawner::Executor;
use anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::MessagePipe;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::wasm::HackRf;
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
    Lna(u16),
    Vga(u16),
}

enum Handle {
    None,
    Flowgraph {
        flowgraph: FlowgraphHandle,
        source: BlockId,
    },
}

pub enum WorkerUpdate {
    Started {
        flowgraph: FlowgraphHandle,
        source: BlockId,
    },
}

pub struct Worker {
    started: bool,
    handle: Handle,
    pending_freq: Option<u64>,
    pending_lna: Option<u16>,
    pending_vga: Option<u16>,
}

impl gloo_worker::Worker for Worker {
    type Input = WorkerMessage;
    type Message = WorkerUpdate;
    type Output = Frame;

    fn create(_scope: &WorkerScope<Self>) -> Self {
        Executor::init_wasm_bindgen().unwrap();
        Self {
            started: false,
            handle: Handle::None,
            pending_freq: None,
            pending_lna: None,
            pending_vga: None,
        }
    }

    fn update(&mut self, _scope: &WorkerScope<Self>, msg: Self::Message) {
        match msg {
            WorkerUpdate::Started { flowgraph, source } => {
                self.handle = Handle::Flowgraph {
                    flowgraph: flowgraph.clone(),
                    source,
                };

                if let Some(freq) = self.pending_freq.take() {
                    post_freq(flowgraph.clone(), source, freq);
                }
                if let Some(lna) = self.pending_lna.take() {
                    post_u16(flowgraph.clone(), source, "lna", lna);
                }
                if let Some(vga) = self.pending_vga.take() {
                    post_u16(flowgraph, source, "vga", vga);
                }
            }
        }
    }

    fn received(&mut self, scope: &WorkerScope<Self>, msg: Self::Input, id: HandlerId) {
        match msg {
            WorkerMessage::Start => {
                if self.started {
                    return;
                }
                self.started = true;
                let scope = scope.clone();
                spawn_local(async move {
                    if let Err(e) = async move {
                        let mut fg = Flowgraph::new();

                        let src = fg.add(HackRf::new());
                        let source = src.id();

                        let mut last: Complex32 = Complex32::new(0.0, 0.0);
                        let mut iir: f32 = 0.0;
                        let alpha = 0.00016;
                        let avg = Apply::new(move |i: &Complex32| -> f32 {
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

                        let (tx_frame, rx_frame) = mpsc::channel::<Pmt>(100);
                        let message_pipe = MessagePipe::new(tx_frame);

                        connect!(fg, src > avg > mm > decoder;
                                     mac > snk;
                                     decoder | rx.mac;
                                     mac.rxed | message_pipe);

                        let rt = Runtime::new();

                        let running = rt.start_async(fg).await?;
                        let (task, handle) = running.split();
                        scope.send_message(WorkerUpdate::Started {
                            flowgraph: handle,
                            source,
                        });
                        spawn_local(async move {
                            if let Err(e) = task.await {
                                info!("ZigBee flowgraph terminated with error: {:?}", e);
                            }
                        });

                        while let Some(x) = rx_frame.recv().await {
                            match x {
                                Pmt::Blob(data) => scope.respond(id, Frame::new(data)),
                                e => info!("ZigBee worker received unexpected PMT: {:?}", e),
                            }
                        }
                        Result::<()>::Ok(())
                    }
                    .await
                    {
                        info!("ZigBee worker: startup/data task failed: {:?}", e);
                    }
                });
            }
            WorkerMessage::Freq(f) => match &mut self.handle {
                Handle::None => {
                    self.pending_freq = Some(f);
                }
                Handle::Flowgraph { flowgraph, source } => {
                    post_freq(flowgraph.clone(), *source, f);
                }
            },
            WorkerMessage::Lna(gain) => match &mut self.handle {
                Handle::None => {
                    self.pending_lna = Some(gain);
                }
                Handle::Flowgraph { flowgraph, source } => {
                    post_u16(flowgraph.clone(), *source, "lna", gain);
                }
            },
            WorkerMessage::Vga(gain) => match &mut self.handle {
                Handle::None => {
                    self.pending_vga = Some(gain);
                }
                Handle::Flowgraph { flowgraph, source } => {
                    post_u16(flowgraph.clone(), *source, "vga", gain);
                }
            },
        }
    }
}

fn post_freq(flowgraph: FlowgraphHandle, source: BlockId, freq: u64) {
    spawn_local(async move {
        if let Err(e) = flowgraph.post(source, "freq", Pmt::U64(freq)).await {
            info!("failed to set source frequency: {:?}", e);
        }
    });
}

fn post_u16(flowgraph: FlowgraphHandle, source: BlockId, handler: &'static str, value: u16) {
    spawn_local(async move {
        if let Err(e) = flowgraph
            .post(source, handler, Pmt::U64(value as u64))
            .await
        {
            info!("failed to set source {}: {:?}", handler, e);
        }
    });
}

#[allow(dead_code)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Frame {
    dst_addr: String,
    dst_pan: String,
    payload: String,
}

impl Frame {
    fn new(data: Vec<u8>) -> Self {
        // The decoder forwards the MAC frame without PHY preamble/SFD/length.
        // These frames use a 7-byte header: FCF, sequence, destination PAN/address.
        const HEADER_LEN: usize = 7;
        // The current decoder output still carries the two-byte FCS plus two
        // trailing bytes from the demodulated frame tail.
        const TRAILER_LEN: usize = 4;

        let dst_pan = get_u16(&data, 3)
            .map(|v| format!("{v:#06x}"))
            .unwrap_or_else(|| "n/a".to_string());
        let dst_addr = get_u16(&data, 5)
            .map(|v| format!("{v:#06x}"))
            .unwrap_or_else(|| "n/a".to_string());
        let payload = if data.len() >= HEADER_LEN + TRAILER_LEN {
            String::from_utf8_lossy(&data[HEADER_LEN..data.len() - TRAILER_LEN]).to_string()
        } else {
            String::new()
        };

        Frame {
            dst_addr,
            dst_pan,
            payload,
        }
    }
}

fn get_u16(data: &[u8], offset: usize) -> Option<u16> {
    data.get(offset..offset + 2)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u16::from_le_bytes)
}
