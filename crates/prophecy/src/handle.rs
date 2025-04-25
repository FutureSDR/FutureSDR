use std::time::Duration;

use futuresdr::runtime;
use futuresdr_types::FlowgraphDescription;
use futuresdr_types::Pmt;
use futuresdr_types::PortId;
use gloo_net::http::Request;
use leptos::logging::*;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::Error;

pub fn get_flowgraph_handle(
    rt: RuntimeHandle,
    flowgraph_id: usize,
) -> Result<ReadSignal<Option<FlowgraphHandle>>, Error> {
    let (fg_handle, set_fg_handle) = signal(None);

    spawn_local(async move {
        match rt.get_flowgraph(flowgraph_id).await {
            Ok(fg) => set_fg_handle(Some(fg)),
            Err(e) => warn!("Error connecting to the flowgraph ({:?}, {})", rt, e),
        }
    });

    Ok(fg_handle)
}

pub fn call_periodically(
    fg: Signal<Option<FlowgraphHandle>>,
    interval: Duration,
    block_id: usize,
    handler: impl Into<PortId>,
    pmt: Pmt,
) {
    let handler = handler.into().clone();
    Effect::new(move |started: Option<bool>| {
        let pmt = pmt.clone();
        let handler = handler.clone();
        match fg.get() {
            Some(mut fg) => {
                if !matches!(started, Some(true)) {
                    log!("Starting to call block {} handler {:?}", block_id, &handler);
                    spawn_local(async move {
                        loop {
                            if fg
                                .call(block_id, handler.clone(), pmt.clone())
                                .await
                                .is_err()
                            {
                                log!("Stopping to call block {} handler {:?}", block_id, &handler);
                                break;
                            }
                            gloo_timers::future::sleep(interval).await;
                        }
                    });
                }
                true
            }
            None => false,
        }
    });
}

pub fn poll_periodically(
    fg: Signal<Option<FlowgraphHandle>>,
    interval: Duration,
    block_id: usize,
    handler: impl Into<PortId>,
    pmt: Pmt,
) -> ReadSignal<Pmt> {
    let (res, set_res) = signal(Pmt::Null);
    let handler = handler.into().clone();
    Effect::new(move |started: Option<bool>| {
        let pmt = pmt.clone();
        let handler = handler.clone();
        match fg.get() {
            Some(mut fg) => {
                if !matches!(started, Some(true)) {
                    log!("Starting to poll block {} handler {:?}", block_id, &handler);
                    spawn_local(async move {
                        loop {
                            match fg.callback(block_id, handler.clone(), pmt.clone()).await {
                                Ok(p) => {
                                    set_res(p);
                                }
                                Err(e) => {
                                    log!(
                                        "Stopping to poll block {} handler {:?}. Error {:?}",
                                        block_id,
                                        &handler,
                                        e
                                    );
                                    break;
                                }
                            }
                            gloo_timers::future::sleep(interval).await;
                        }
                    });
                }
                true
            }
            None => false,
        }
    });
    res
}

/// Reference to a FutureSDR Runtime
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeHandle {
    /// FutureSDR running in the browser
    Web(runtime::RuntimeHandle),
    /// FutureSDR running on a host
    Remote(String),
}

impl RuntimeHandle {
    pub fn from_url(u: impl Into<String>) -> Self {
        let mut u = u.into();
        if !u.ends_with('/') {
            u += "/";
        }
        Self::Remote(u)
    }
    pub fn from_handle(h: runtime::RuntimeHandle) -> Self {
        Self::Web(h)
    }
    pub async fn get_flowgraphs(&self) -> Result<Vec<usize>, Error> {
        match self {
            Self::Remote(u) => Ok(Request::get(&format!("{u}api/fg/"))
                .send()
                .await?
                .json()
                .await?),
            Self::Web(h) => Ok(h.get_flowgraphs()),
        }
    }
    pub async fn get_flowgraph(&self, id: usize) -> Result<FlowgraphHandle, Error> {
        match self {
            Self::Remote(u) => Ok(FlowgraphHandle::Remote(format!("{u}api/fg/{id}/"))),
            Self::Web(h) => Ok(FlowgraphHandle::Web(
                h.get_flowgraph(id).ok_or(Error::FlowgraphId(id))?,
            )),
        }
    }
}

/// Reference to a FutureSDR Flowgraph
#[derive(Debug, Clone, PartialEq)]
pub enum FlowgraphHandle {
    /// FutureSDR running in the browser
    Web(runtime::FlowgraphHandle),
    /// FutureSDR running on a host
    Remote(String),
}

impl FlowgraphHandle {
    pub fn from_url(u: impl Into<String>) -> Self {
        let mut u = u.into();
        if !u.ends_with('/') {
            u += "/";
        }
        Self::Remote(u)
    }
    pub fn from_handle(h: runtime::FlowgraphHandle) -> Self {
        Self::Web(h)
    }
    pub async fn description(&mut self) -> Result<FlowgraphDescription, Error> {
        match self {
            Self::Remote(u) => Ok(Request::get(u).send().await?.json().await?),
            Self::Web(h) => Ok(h.description().await?),
        }
    }
    pub async fn call(
        &mut self,
        block_id: usize,
        handler: impl Into<PortId>,
        pmt: Pmt,
    ) -> Result<(), Error> {
        match self {
            Self::Remote(u) => {
                let _ = gloo_net::http::Request::post(&format!(
                    "{}block/{}/call/{}/",
                    u,
                    block_id,
                    handler.into()
                ))
                .header("Content-Type", "application/json")
                .body(serde_json::to_string(&pmt)?)?
                .send()
                .await?;
                Ok(())
            }
            Self::Web(h) => Ok(h.call(block_id, handler, pmt).await?),
        }
    }
    pub async fn callback(
        &mut self,
        block_id: usize,
        handler: impl Into<PortId>,
        pmt: Pmt,
    ) -> Result<Pmt, Error> {
        match self {
            Self::Remote(u) => {
                let response = gloo_net::http::Request::post(&format!(
                    "{}block/{}/call/{}/",
                    u,
                    block_id,
                    handler.into()
                ))
                .header("Content-Type", "application/json")
                .body(serde_json::to_string(&pmt)?)?
                .send()
                .await?;
                if response.ok() {
                    Ok(serde_json::from_str(&response.text().await?)?)
                } else {
                    Err(Error::Gloo(format!("Request failed {response:?}")))
                }
            }
            Self::Web(h) => Ok(h.callback(block_id, handler, pmt).await?),
        }
    }
}
