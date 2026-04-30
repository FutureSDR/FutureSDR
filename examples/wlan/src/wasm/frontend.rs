use any_spawner::Executor;
use futuresdr::runtime::FlowgraphDescription;
use futuresdr::runtime::FlowgraphId;
use futuresdr::runtime::Pmt;
use leptos::html::Span;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::wasm_bindgen::JsCast;
use leptos::web_sys::HtmlInputElement;
use leptos::web_sys::KeyboardEvent;
use prophecy::ConstellationSinkDensity;
use prophecy::FlowgraphCanvas;
use prophecy::FlowgraphHandle;
use prophecy::FlowgraphTable;
use prophecy::ListSelector;
use prophecy::PmtEditor;
use prophecy::RuntimeHandle;

fn find_seify_source_block_id(desc: &FlowgraphDescription) -> Option<usize> {
    let seify_blocks: Vec<_> = desc
        .blocks
        .iter()
        .filter(|b| b.type_name.to_ascii_lowercase().contains("seify"))
        .collect();

    // Prefer the Seify block that exposes the expected radio-control handlers.
    seify_blocks
        .iter()
        .find(|b| {
            b.message_inputs.iter().any(|h| h == "sample_rate")
                && b.message_inputs.iter().any(|h| h == "freq")
                && b.message_inputs.iter().any(|h| h == "gain")
        })
        .or_else(|| {
            // Fallback to type-name based source match.
            seify_blocks
                .iter()
                .find(|b| b.type_name.eq_ignore_ascii_case("SeifySource"))
        })
        .or_else(|| {
            seify_blocks
                .iter()
                .find(|b| b.type_name.to_ascii_lowercase().contains("source"))
        })
        .or_else(|| seify_blocks.first())
        .map(|b| b.id.0)
}

#[derive(Clone, Debug, PartialEq)]
struct MessageInputTarget {
    block_id: usize,
    block_name: String,
    handler: String,
    source: &'static str,
}

#[component]
pub fn Wlan(fg_handle: FlowgraphHandle) -> impl IntoView {
    let fg_desc = {
        let fg_handle = fg_handle.clone();
        LocalResource::new(move || {
            let fg_handle = fg_handle.clone();
            async move {
                if let Ok(desc) = fg_handle.describe().await {
                    return Some(desc);
                }
                None
            }
        })
    };

    let (width, _set_width) = signal(2.0f32);
    let source_block_id = Memo::new(move |_| {
        fg_desc
            .get()
            .and_then(|x| x)
            .and_then(|desc| find_seify_source_block_id(&desc))
    });

    let gain_label = NodeRef::<Span>::new();
    let (target, set_target) = signal(None::<MessageInputTarget>);
    let (submit_error, set_submit_error) = signal(None::<String>);
    let (submitting, set_submitting) = signal(false);
    let _esc_listener = window_event_listener(leptos::ev::keydown, move |ev: KeyboardEvent| {
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
    let fg_for_submit = fg_handle.clone();
    let on_submit_pmt = Callback::new(move |pmt: Pmt| {
        let selected = target.get_untracked();
        if let Some(selected) = selected {
            set_submitting(true);
            set_submit_error(None);
            let fg = fg_for_submit.clone();
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
    let sample_rate_initialized = RwSignal::new(false);
    let fg_for_sample_rate_init = fg_handle.clone();
    let fg_for_channel_select = fg_handle.clone();
    Effect::new(move |_| {
        if sample_rate_initialized.get_untracked() {
            return;
        }
        if let Some(source) = source_block_id.get() {
            sample_rate_initialized.set(true);
            let fg = fg_for_sample_rate_init.clone();
            spawn_local(async move {
                let _ = fg.post(source, "sample_rate", Pmt::F64(20e6)).await;
            });
        }
    });

    view! {
        <div class="bg-slate-800 border border-slate-700 rounded-xl m-4 p-5 shadow-lg">
            <div class="flex items-center justify-between mb-4">
                <h2 class="text-white text-lg font-semibold">"Radio Controls"</h2>
                <span class="text-xs text-slate-400">
                    {move || {
                        source_block_id
                            .get()
                            .map(|id| format!("source block: {id}"))
                            .unwrap_or_else(|| "source block: n/a".to_string())
                    }}
                </span>
            </div>
            <div class="grid grid-cols-1 md:grid-cols-3 xl:grid-cols-3 gap-4 items-start">
                <div class="bg-slate-900 border border-slate-700 rounded-lg p-3 h-40 flex flex-col">
                    <div class="text-slate-300 text-sm mb-2">"Sample Rate"</div>
                    <div class="flex flex-col gap-2 text-slate-200 text-sm">
                        <label class="inline-flex items-center gap-2 cursor-pointer">
                            <input
                                type="radio"
                                name="wlan-sample-rate"
                                on:change={
                                    let fg_handle = fg_handle.clone();
                                    move |_| {
                                        let fg_handle = fg_handle.clone();
                                        if let Some(source_block_id) = source_block_id.get_untracked() {
                                            spawn_local(async move {
                                                let _ = fg_handle.post(source_block_id, "sample_rate", Pmt::F64(5e6)).await;
                                            });
                                        }
                                    }
                                }
                            />
                            <span>"5 MHz"</span>
                        </label>
                        <label class="inline-flex items-center gap-2 cursor-pointer">
                            <input
                                type="radio"
                                name="wlan-sample-rate"
                                on:change={
                                    let fg_handle = fg_handle.clone();
                                    move |_| {
                                        let fg_handle = fg_handle.clone();
                                        if let Some(source_block_id) = source_block_id.get_untracked() {
                                            spawn_local(async move {
                                                let _ = fg_handle.post(source_block_id, "sample_rate", Pmt::F64(10e6)).await;
                                            });
                                        }
                                    }
                                }
                            />
                            <span>"10 MHz"</span>
                        </label>
                        <label class="inline-flex items-center gap-2 cursor-pointer">
                            <input
                                type="radio"
                                name="wlan-sample-rate"
                                checked=true
                                on:change={
                                    let fg_handle = fg_handle.clone();
                                    move |_| {
                                        let fg_handle = fg_handle.clone();
                                        if let Some(source_block_id) = source_block_id.get_untracked() {
                                            spawn_local(async move {
                                                let _ = fg_handle.post(source_block_id, "sample_rate", Pmt::F64(20e6)).await;
                                            });
                                        }
                                    }
                                }
                            />
                            <span>"20 MHz"</span>
                        </label>
                    </div>
                </div>
                <div class="bg-slate-900 border border-slate-700 rounded-lg p-3 h-40 flex flex-col">
                    <div class="text-slate-300 text-sm mb-2">"WLAN Channel"</div>
                    {move || {
                        match source_block_id.get() {
                            Some(source_id) => view! {
                                <ListSelector
                                    fg_handle=fg_for_channel_select.clone()
                                    block_id=source_id
                                    handler="freq"
                                    select_class="w-full rounded bg-slate-800 border border-slate-600 text-slate-100 px-2 py-2 text-sm"
                                    values=[
                    // 11g
                    ("1".to_string(),	Pmt::F64(2412e6)),
                    ("2".to_string(),	Pmt::F64(2417e6)),
                    ("3".to_string(),	Pmt::F64(2422e6)),
                    ("4".to_string(),	Pmt::F64(2427e6)),
                    ("5".to_string(),	Pmt::F64(2432e6)),
                    ("6".to_string(),	Pmt::F64(2437e6)),
                    ("7".to_string(),	Pmt::F64(2442e6)),
                    ("8".to_string(),	Pmt::F64(2447e6)),
                    ("9".to_string(),	Pmt::F64(2452e6)),
                    ("10".to_string(),	Pmt::F64(2457e6)),
                    ("11".to_string(),	Pmt::F64(2462e6)),
                    ("12".to_string(),	Pmt::F64(2467e6)),
                    ("13".to_string(),	Pmt::F64(2472e6)),
                    ("14".to_string(),	Pmt::F64(2484e6)),
                    // 11a
                    ("34".to_string(),	Pmt::F64(5170e6)),
                    ("36".to_string(),	Pmt::F64(5180e6)),
                    ("38".to_string(),	Pmt::F64(5190e6)),
                    ("40".to_string(),	Pmt::F64(5200e6)),
                    ("42".to_string(),	Pmt::F64(5210e6)),
                    ("44".to_string(),	Pmt::F64(5220e6)),
                    ("46".to_string(),	Pmt::F64(5230e6)),
                    ("48".to_string(),	Pmt::F64(5240e6)),
                    ("50".to_string(),	Pmt::F64(5250e6)),
                    ("52".to_string(),	Pmt::F64(5260e6)),
                    ("54".to_string(),	Pmt::F64(5270e6)),
                    ("56".to_string(),	Pmt::F64(5280e6)),
                    ("58".to_string(),	Pmt::F64(5290e6)),
                    ("60".to_string(),	Pmt::F64(5300e6)),
                    ("62".to_string(),	Pmt::F64(5310e6)),
                    ("64".to_string(),	Pmt::F64(5320e6)),
                    ("100".to_string(),	Pmt::F64(5500e6)),
                    ("102".to_string(),	Pmt::F64(5510e6)),
                    ("104".to_string(),	Pmt::F64(5520e6)),
                    ("106".to_string(),	Pmt::F64(5530e6)),
                    ("108".to_string(),	Pmt::F64(5540e6)),
                    ("110".to_string(),	Pmt::F64(5550e6)),
                    ("112".to_string(),	Pmt::F64(5560e6)),
                    ("114".to_string(),	Pmt::F64(5570e6)),
                    ("116".to_string(),	Pmt::F64(5580e6)),
                    ("118".to_string(),	Pmt::F64(5590e6)),
                    ("120".to_string(),	Pmt::F64(5600e6)),
                    ("122".to_string(),	Pmt::F64(5610e6)),
                    ("124".to_string(),	Pmt::F64(5620e6)),
                    ("126".to_string(),	Pmt::F64(5630e6)),
                    ("128".to_string(),	Pmt::F64(5640e6)),
                    ("132".to_string(),	Pmt::F64(5660e6)),
                    ("134".to_string(),	Pmt::F64(5670e6)),
                    ("136".to_string(),	Pmt::F64(5680e6)),
                    ("138".to_string(),	Pmt::F64(5690e6)),
                    ("140".to_string(),	Pmt::F64(5700e6)),
                    ("142".to_string(),	Pmt::F64(5710e6)),
                    ("144".to_string(),	Pmt::F64(5720e6)),
                    ("149".to_string(),	Pmt::F64(5745e6)),
                    ("151".to_string(),	Pmt::F64(5755e6)),
                    ("153".to_string(),	Pmt::F64(5765e6)),
                    ("155".to_string(),	Pmt::F64(5775e6)),
                    ("157".to_string(),	Pmt::F64(5785e6)),
                    ("159".to_string(),	Pmt::F64(5795e6)),
                    ("161".to_string(),	Pmt::F64(5805e6)),
                    ("165".to_string(),	Pmt::F64(5825e6)),
                    //11p
                    ("172".to_string(),	Pmt::F64(5860e6)),
                    ("174".to_string(),	Pmt::F64(5870e6)),
                    ("176".to_string(),	Pmt::F64(5880e6)),
                    ("178".to_string(),	Pmt::F64(5890e6)),
                    ("180".to_string(),	Pmt::F64(5900e6)),
                    ("182".to_string(),	Pmt::F64(5910e6)),
                    ("184".to_string(),	Pmt::F64(5920e6)),
                    ] />
                            }
                                .into_any(),
                            None => view! {
                                <div class="text-slate-500 text-sm mt-2">
                                    "Seify source block not found."
                                </div>
                            }
                                .into_any(),
                        }
                    }}
                </div>
                <div class="bg-slate-900 border border-slate-700 rounded-lg p-3 h-40 flex flex-col">
                    <div class="text-slate-300 text-sm mb-2">"RF Gain"</div>
                    <input type="range" min="0" max="80" value="60" class="w-full align-middle accent-cyan-400"
                        on:change= {
                            let fg_handle = fg_handle.clone();
                            move |v| {
                                let target = v.target().unwrap();
                                let input : HtmlInputElement = target.dyn_into().unwrap();
                                gain_label.get().unwrap().set_inner_text(&format!("gain: {} dB", input.value()));
                                let gain : f64 = input.value().parse().unwrap();
                                let p = Pmt::F64(gain);
                                let fg_handle = fg_handle.clone();
                                if let Some(source_block_id) = source_block_id.get_untracked() {
                                    spawn_local(async move {
                                        let _ = fg_handle.post(source_block_id, "gain", p).await;
                                    });
                                }
                    }} />
                    <span class="text-slate-100 text-sm block mt-2" node_ref=gain_label>"gain: 60 dB"</span>
                </div>
            </div>
        </div>

        <div class="bg-slate-800 border border-slate-700 rounded-xl m-4 p-4 shadow-lg">
            <h2 class="text-white text-lg font-semibold mb-3">"Constellation"</h2>
            <ConstellationSinkDensity width=width />
        </div>

        <div class="bg-slate-800 border border-slate-700 rounded-xl m-4 p-4 shadow-lg">
            <h2 class="text-white text-lg font-semibold mb-3">"Flowgraph"</h2>
            {move || {
                match fg_desc.get() {
                    Some(Some(desc)) => view! {
                        <FlowgraphCanvas
                            fg=desc.clone()
                            on_message_input_click=on_canvas_message_input_click
                        />
                        <div class="-mx-4">
                            <FlowgraphTable
                                fg=desc
                                on_message_input_click=on_table_message_input_click
                            />
                        </div>
                    }
                        .into_any(),
                    _ => view! {}.into_any(),
                }
            }}
        </div>
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
pub fn Gui() -> impl IntoView {
    // let rt_url = window().location().origin().unwrap();
    // let rt_handle = RuntimeHandle::from_url(rt_url);
    let rt_handle = RuntimeHandle::from_url("http://127.0.0.1:1337");

    let fg_handle = LocalResource::new(move || {
        let rt_handle = rt_handle.clone();
        async move {
            if let Ok(fg) = rt_handle.get_flowgraph(FlowgraphId(0)).await {
                Some(fg)
            } else {
                None
            }
        }
    });

    view! {
        <div class="min-h-screen bg-slate-900">
            <header class="bg-slate-800 border-b border-slate-700 shadow-lg">
                <div class="flex items-center gap-3 px-4 py-3">
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
                        <span class="text-cyan-400 font-light text-base ml-1.5">"WLAN"</span>
                        <div class="text-xs text-slate-400">"Receiver Control Panel"</div>
                    </div>
                </div>
            </header>

            {move || {
                match fg_handle.get() {
                    Some(wrapped) => match wrapped {
                        Some(handle) => view! { <Wlan fg_handle=handle /> }.into_any(),
                        _ => view! {
                            <div class="text-slate-300 p-6">"Failed to attach flowgraph."</div>
                        }
                            .into_any(),
                    }
                    _ => view! {
                        <div class="text-slate-300 p-6">"Connecting..."</div>
                    }
                        .into_any(),
                }
            }}
        </div>
    }
}

pub fn frontend() {
    console_error_panic_hook::set_once();
    Executor::init_wasm_bindgen().unwrap();
    mount_to_body(|| view! { <Gui /> })
}
