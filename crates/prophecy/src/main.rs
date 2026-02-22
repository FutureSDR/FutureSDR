#![allow(unused_imports)]
use any_spawner::Executor;
use futuresdr::futures::StreamExt;
use futuresdr::runtime::Pmt;
use futuresdr_types::FlowgraphId;
use gloo_net::websocket::Message;
use gloo_net::websocket::futures::WebSocket;
use leptos::html::Input;
use leptos::html::Span;
use leptos::logging::*;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::wasm_bindgen::JsCast;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use web_sys::HtmlInputElement;

use prophecy::FlowgraphCanvas;
use prophecy::FlowgraphHandle;
use prophecy::FlowgraphTable;
use prophecy::ListSelector;
use prophecy::Pmt;
use prophecy::PmtEditor;
use prophecy::PmtInput;
use prophecy::PmtInputList;
use prophecy::RadioSelector;
use prophecy::RuntimeHandle;
use prophecy::Slider;
use prophecy::TimeSink;
use prophecy::TimeSinkMode;
use prophecy::Waterfall;
use prophecy::WaterfallMode;
use prophecy::poll_periodically;

#[derive(Clone, Debug, PartialEq)]
struct MessageInputTarget {
    block_id: usize,
    block_name: String,
    handler: String,
    source: &'static str,
}

#[component]
/// Textual Flowgraph Description
pub fn Flowgraph(fg_handle: FlowgraphHandle) -> impl IntoView {
    let fg_desc = {
        let fg_handle = fg_handle.clone();
        LocalResource::new(move || {
            let mut fg_handle = fg_handle.clone();
            async move { fg_handle.description().await.ok() }
        })
    };

    let (target, set_target) = signal(None::<MessageInputTarget>);
    let (submit_error, set_submit_error) = signal(None::<String>);
    let (submitting, set_submitting) = signal(false);
    let _esc_listener =
        window_event_listener(leptos::ev::keydown, move |ev: web_sys::KeyboardEvent| {
            if ev.key() == "Escape" && target.get_untracked().is_some() {
                set_target(None);
            }
        });

    let on_canvas_message_input_click = Callback::new(move |(block_id, block_name, handler)| {
        set_submit_error(None);
        set_target(Some(MessageInputTarget {
            block_id,
            block_name,
            handler,
            source: "canvas",
        }));
    });

    let on_table_message_input_click = Callback::new(move |(block_id, block_name, handler)| {
        set_submit_error(None);
        set_target(Some(MessageInputTarget {
            block_id,
            block_name,
            handler,
            source: "table",
        }));
    });

    let on_submit_pmt = Callback::new(move |pmt: Pmt| {
        let selected = target.get_untracked();
        if let Some(selected) = selected {
            set_submitting(true);
            set_submit_error(None);
            let mut fg = fg_handle.clone();
            spawn_local(async move {
                let result = fg
                    .put_message_input(selected.block_id, selected.handler.clone(), pmt)
                    .await;
                set_submitting(false);
                match result {
                    Ok(()) => set_target(None),
                    Err(e) => set_submit_error(Some(format!("failed to send PMT: {e}"))),
                }
            });
        }
    });

    view! {
        {move || match fg_desc.get() {
            Some(wrapped) => {
                match wrapped {
                    Some(data) => {
                        view! {
                            <div>
                                <FlowgraphCanvas
                                    fg=data.clone()
                                    on_message_input_click=on_canvas_message_input_click
                                />
                                <FlowgraphTable
                                    fg=data
                                    on_message_input_click=on_table_message_input_click
                                />
                            </div>
                        }
                            .into_any()
                    }
                    None => "Flowgraph Handle not set".into_any(),
                }
            }
            None => {
                view! { <p>"Connecting..."</p> }
                    .into_any()
            }
        }}
        {move || target
            .get()
            .map(|current| {
                view! {
                    <div
                        class="fixed inset-0 z-50 bg-black/70 flex items-center justify-center p-4"
                        on:click=move |_| set_target(None)
                    >
                        <div
                            class="w-full max-w-2xl rounded-lg bg-slate-900 border border-slate-700 p-4"
                            on:click=move |ev| ev.stop_propagation()
                        >
                            <div class="flex items-center justify-between">
                                <div>
                                    <h3 class="text-white text-lg font-semibold">"Send PMT"</h3>
                                    <p class="text-slate-300 text-sm">
                                        {format!(
                                            "{} -> block {} ({}) / handler '{}'",
                                            current.source,
                                            current.block_id,
                                            current.block_name,
                                            current.handler
                                        )}
                                    </p>
                                </div>
                                <button
                                    class="rounded bg-slate-700 hover:bg-slate-600 px-3 py-1 text-sm text-white"
                                    on:click=move |_| set_target(None)
                                    disabled=submitting
                                >
                                    "Close"
                                </button>
                            </div>
                            <div class="mt-3">
                                <PmtEditor
                                    on_submit=on_submit_pmt
                                    disabled=submitting()
                                    select_class="w-full rounded bg-slate-800 text-white px-2 py-2"
                                    input_class="w-full h-32 rounded bg-slate-800 text-white px-2 py-2 font-mono"
                                    error_class="text-red-400 text-sm"
                                    button_class="rounded bg-blue-600 hover:bg-blue-500 text-white px-3 py-2"
                                    button_text=if submitting() {
                                        "Sending...".to_string()
                                    } else {
                                        "Send".to_string()
                                    }
                                />
                            </div>
                            <div class="mt-2 text-red-400 text-sm">
                                {move || submit_error.get().unwrap_or_default()}
                            </div>
                        </div>
                    </div>
                }
            })}
    }
}

#[component]
/// Top bar: runtime URL input and flowgraph selector dropdown
pub fn RuntimeControl() -> impl IntoView {
    let default_url = window().location().origin().unwrap();
    let (rt_handle, set_rt_handle) = signal(RuntimeHandle::from_url(default_url.clone()));
    let (fg_handle, set_fg_handle) = signal(None::<FlowgraphHandle>);
    let fg_ids: RwSignal<Vec<FlowgraphId>> = RwSignal::new(vec![]);

    // Re-fetch flowgraph list whenever rt_handle changes; auto-connect to flowgraph 0
    Effect::new(move |_| {
        let rt = rt_handle.get();
        spawn_local(async move {
            let fgs = rt.get_flowgraphs().await.unwrap_or_default();
            fg_ids.set(fgs.clone());
            if let Some(&first) = fgs.first()
                && let Ok(fg) = rt.get_flowgraph(first).await
            {
                set_fg_handle(Some(fg));
            }
        });
    });

    // Connecting to a flowgraph on dropdown change
    let on_fg_change = move |ev: web_sys::Event| {
        let val = event_target_value(&ev);
        if let Ok(idx) = val.parse::<usize>() {
            let ids = fg_ids.get_untracked();
            if let Some(&id) = ids.get(idx) {
                spawn_local(async move {
                    if let Ok(fg) = rt_handle.get_untracked().get_flowgraph(id).await {
                        set_fg_handle(Some(fg));
                    }
                });
            }
        }
    };

    // Update the runtime handle when Enter is pressed in the URL field
    let url_ref = NodeRef::<Input>::new();
    let on_url_keydown = move |ev: web_sys::KeyboardEvent| {
        if ev.key() == "Enter"
            && let Some(input) = url_ref.get()
        {
            set_rt_handle(RuntimeHandle::from_url(input.value()));
        }
    };

    let connected = move || fg_handle.get().is_some();

    view! {
        <header class="bg-slate-800 border-b border-slate-700 shadow-lg">
            <div class="flex items-center gap-4 px-4 py-3">
                // Brand
                <div class="flex items-center gap-2.5 mr-auto select-none">
                    <svg
                        class="w-7 h-7 text-cyan-400 shrink-0"
                        viewBox="0 0 28 28"
                        fill="none"
                        xmlns="http://www.w3.org/2000/svg"
                    >
                        <path
                            d="M1 14 Q4 6, 7 14 Q10 22, 13 14 Q16 6, 19 14 Q22 22, 25 14 Q27 9, 28 14"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            fill="none"
                        />
                    </svg>
                    <div class="leading-tight">
                        <span class="text-white font-semibold tracking-tight text-base">"FutureSDR"</span>
                        <span class="text-cyan-400 font-light text-base ml-1.5">"Prophecy"</span>
                    </div>
                </div>

                // Connection status
                <div class="flex items-center gap-1.5 text-xs font-medium">
                    <span class={move || if connected() {
                        "w-2 h-2 rounded-full bg-emerald-400 shadow-[0_0_6px_1px_rgba(52,211,153,0.6)]"
                    } else {
                        "w-2 h-2 rounded-full bg-slate-500"
                    }} />
                    <span class={move || if connected() {
                        "text-emerald-400"
                    } else {
                        "text-slate-400"
                    }}>
                        {move || if connected() { "Connected" } else { "Disconnected" }}
                    </span>
                </div>

                // Separator
                <div class="w-px h-5 bg-slate-600" />

                // Runtime URL input
                <div class="flex items-center gap-2">
                    <label class="text-slate-400 text-xs uppercase tracking-wider whitespace-nowrap">
                        "Runtime"
                    </label>
                    <input
                        node_ref=url_ref
                        type="text"
                        value=default_url
                        class="bg-slate-700 text-white text-sm rounded-md px-2.5 py-1.5 w-56 focus:outline-none focus:ring-1 focus:ring-cyan-500 border border-slate-600 placeholder-slate-500"
                        placeholder="http://localhost:1337"
                        on:keydown=on_url_keydown
                    />
                </div>

                // Flowgraph selector
                <div class="flex items-center gap-2">
                    <label class="text-slate-400 text-xs uppercase tracking-wider whitespace-nowrap">
                        "Flowgraph"
                    </label>
                    <select
                        class="bg-slate-700 text-white text-sm rounded-md px-2.5 py-1.5 focus:outline-none focus:ring-1 focus:ring-cyan-500 border border-slate-600 disabled:opacity-40"
                        on:change=on_fg_change
                        disabled=move || fg_ids.get().is_empty()
                    >
                        {move || {
                            let ids = fg_ids.get();
                            if ids.is_empty() {
                                view! {
                                    <option value="">"No flowgraphs"</option>
                                }.into_any()
                            } else {
                                ids.into_iter()
                                    .enumerate()
                                    .map(|(i, id)| {
                                        view! {
                                            <option value={i.to_string()}>
                                                {format!("Flowgraph #{}", id.0)}
                                            </option>
                                        }
                                    })
                                    .collect::<Vec<_>>()
                                    .into_any()
                            }
                        }}
                    </select>
                </div>
            </div>
        </header>

        {move || match fg_handle.get() {
            Some(h) => view! { <Flowgraph fg_handle=h /> }.into_any(),
            None => "".into_any(),
        }}
    }
}

#[component]
/// Main GUI
pub fn Prophecy() -> impl IntoView {
    view! {
        <div class="min-h-screen bg-slate-900 flex flex-col">
            <RuntimeControl />
        </div>
    }
}

pub fn main() {
    console_error_panic_hook::set_once();
    Executor::init_wasm_bindgen().unwrap();
    mount_to_body(|| view! { <Prophecy /> })
}
