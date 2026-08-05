use futuresdr::blocks::Apply;
use futuresdr::blocks::Fft;
use futuresdr::blocks::FftDirection;
use futuresdr::blocks::MovingAvg;
use futuresdr::blocks::seify::AsyncBuilder;
use futuresdr::runtime::dev::prelude::*;
use futuresdr::seify::AsyncRegistry;
use prophecy::FlowgraphCanvas;
use prophecy::FlowgraphTable;
use prophecy::PmtEditor;
use prophecy::SeifySource;
use prophecy::TimeSink;
use prophecy::TimeSinkMode;
use prophecy::Waterfall;
use prophecy::WaterfallMode;
use prophecy::leptos;
use prophecy::leptos::prelude::*;
use prophecy::leptos::task::spawn_local;
use prophecy::leptos::web_sys::KeyboardEvent;
use wasm_bindgen_futures::js_sys;

const FFT_SIZE: usize = 2048;

#[derive(Clone, Debug, PartialEq)]
struct MessageInputTarget {
    block_id: usize,
    block_name: String,
    handler: String,
    source: &'static str,
}

#[component]
fn PowerScale(
    min: ReadSignal<f32>,
    set_min: WriteSignal<f32>,
    max: ReadSignal<f32>,
    set_max: WriteSignal<f32>,
) -> impl IntoView {
    view! {
        <div class="basis-full grid grid-cols-1 gap-4 md:grid-cols-2">
            <div class="rounded border border-slate-700 bg-slate-900 p-3">
                <label class="mb-2 block text-sm text-slate-300">"Minimum Power"</label>
                <input
                    type="range"
                    min="-100"
                    max="50"
                    prop:value=move || min.get()
                    class="w-full align-middle"
                    on:input=move |event| {
                        if let Ok(value) = event_target_value(&event).parse() {
                            set_min(value);
                        }
                    }
                />
                <div class="mt-1 text-sm text-white">
                    {move || format!("{} dB", min.get())}
                </div>
            </div>
            <div class="rounded border border-slate-700 bg-slate-900 p-3">
                <label class="mb-2 block text-sm text-slate-300">"Maximum Power"</label>
                <input
                    type="range"
                    min="-40"
                    max="100"
                    prop:value=move || max.get()
                    class="w-full align-middle"
                    on:input=move |event| {
                        if let Ok(value) = event_target_value(&event).parse() {
                            set_max(value);
                        }
                    }
                />
                <div class="mt-1 text-sm text-white">
                    {move || format!("{} dB", max.get())}
                </div>
            </div>
        </div>
    }
}

#[component]
/// Spectrum Widget
pub fn Spectrum(
    handle: prophecy::FlowgraphHandle,
    time_data: ReadSignal<Vec<u8>>,
    waterfall_data: ReadSignal<Vec<u8>>,
    seify_block_id: Option<usize>,
) -> impl IntoView {
    let fg_desc = LocalResource::new({
        let handle = handle.clone();
        move || {
            let handle = handle.clone();
            async move {
                if let Ok(desc) = handle.describe().await {
                    return Some(desc);
                }
                None
            }
        }
    });

    let (min, set_min) = signal(-40.0f32);
    let (max, set_max) = signal(20.0f32);

    let (ctrl, set_ctrl) = signal(true);
    let ctrl_click = move |_| {
        set_ctrl(!ctrl());
    };
    let (target, set_target) = signal(None::<MessageInputTarget>);
    let (submit_error, set_submit_error) = signal(None::<String>);
    let (submitting, set_submitting) = signal(false);
    let _esc_listener = window_event_listener(leptos::ev::keydown, move |ev: KeyboardEvent| {
        if ev.key() == "Escape" && target.try_get_untracked().flatten().is_some() {
            let _ = set_target.try_set(None);
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
    let fg_for_submit = handle.clone();
    let on_submit_pmt = Callback::new(move |pmt: Pmt| {
        if let Some(selected) = target.get_untracked() {
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

    view! {
        <div class="text-white">
            <button class="p-2 m-4 rounded bg-slate-600 hover:bg-slate-800" on:click=ctrl_click>
                Show/Hide Controls
            </button>
        </div>
        <Show when=ctrl>
            <div class="flex flex-row flex-wrap gap-4 p-4 m-4 border-2 rounded-md border-slate-500">
                <PowerScale min=min set_min=set_min max=max set_max=set_max />
                {seify_block_id.map(|block_id| {
                    view! { <SeifySource fg_handle=handle.clone() block_id=block_id /> }
                })}
            </div>
        </Show>
        <div
            class="m-4 border-2 rounded-md border-slate-500"
            style="height: 400px; max-height: 40vh"
        >
            <TimeSink min=min max=max mode=TimeSinkMode::Data(time_data) />
        </div>
        <div
            class="m-4 border-2 rounded-md border-slate-500"
            style="height: 400px; max-height: 40vh"
        >
            <Waterfall min=min max=max mode=WaterfallMode::Data(waterfall_data) />
        </div>
        <div class="m-4 space-y-4">
            {move || {
                fg_desc
                    .get()
                    .map(|x| x.unwrap())
                    .map(|x| {
                        view! {
                            <div class="border-2 rounded-md border-slate-500">
                                <FlowgraphCanvas
                                    fg=x.clone()
                                    on_message_input_click=on_canvas_message_input_click
                                />
                            </div>
                            <div class="border-2 rounded-md border-slate-500 overflow-x-auto">
                                <FlowgraphTable fg=x on_message_input_click=on_table_message_input_click />
                            </div>
                        }
                            .into_any()
                    })
                    .unwrap_or(().into_any());
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
/// Main GUI
pub fn Gui() -> impl IntoView {
    let (handle, set_handle) = signal_local(None);
    let (time_data, set_time_data) = signal(vec![]);
    let (waterfall_data, set_waterfall_data) = signal(vec![]);
    let (seify_block_id, set_seify_block_id) = signal(None::<usize>);
    let (start_error, set_start_error) = signal(None::<String>);

    view! {
        <h1 class="m-4 text-xl text-white">FutureSDR Spectrum</h1>
        {move || {
            match handle.get() {
                Some(handle) => {
                    let handle = prophecy::FlowgraphHandle::from_handle(handle);
                    view! {
                        <Spectrum
                            handle=handle
                            time_data=time_data
                            waterfall_data=waterfall_data
                            seify_block_id=seify_block_id()
                        />
                    }
                        .into_any()
                }
                _ => {
                    view! {
                        <div class="m-4 space-y-4 text-white">
                            <button
                                class="p-2 rounded bg-slate-600 hover:bg-slate-700"
                                on:click=move |_| {
                                    set_start_error(None);
                                    set_seify_block_id(None);
                                    spawn_local({
                                        async move {
                                            if let Err(e) = run(
                                                set_handle,
                                                set_time_data,
                                                set_waterfall_data,
                                                set_seify_block_id,
                                            )
                                            .await
                                            {
                                                let _ = set_start_error.try_set(Some(format!("failed to start flowgraph: {e}")));
                                            }
                                        }
                                    });
                                }
                            >
                                Start
                            </button>
                            <div>"Please Click to Start Flowgraph"</div>
                            <div class="text-red-400">
                                {move || start_error.get().unwrap_or_default()}
                            </div>
                        </div>
                    }
                        .into_any()
                }
            }
        }}
    }
}

pub fn web() {
    console_error_panic_hook::set_once();
    mount_to_body(|| view! { <Gui /> })
}

#[derive(Block)]
pub struct Sink {
    #[input]
    input: slab::Reader<f32>,
    time_data: WriteSignal<Vec<u8>>,
    waterfall_data: WriteSignal<Vec<u8>>,
    last_update_ms: f64,
}

impl Sink {
    pub fn new(time_data: WriteSignal<Vec<u8>>, waterfall_data: WriteSignal<Vec<u8>>) -> Self {
        let mut input = slab::Reader::default();
        input.set_min_items(FFT_SIZE);
        Self {
            input,
            time_data,
            waterfall_data,
            last_update_ms: 0.0,
        }
    }
}

impl Kernel for Sink {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
    ) -> Result<()> {
        let input = self.input.slice();
        // log!("sink len {} io {:?}", input.len(), &io);

        let n_frames = input.len() / FFT_SIZE;
        if n_frames > 0 {
            let now_ms = js_sys::Date::now();
            if now_ms - self.last_update_ms >= 33.0 {
                let offset = (n_frames - 1) * FFT_SIZE;
                let samples = &input[offset..offset + FFT_SIZE];
                let bytes = unsafe {
                    let l = samples.len() * 4;
                    let p = samples.as_ptr();
                    std::slice::from_raw_parts(p as *const u8, l)
                };
                let bytes = Vec::from(bytes);
                let time_disposed = self.time_data.try_set(bytes.clone()).is_some();
                let waterfall_disposed = self.waterfall_data.try_set(bytes).is_some();
                if time_disposed && waterfall_disposed {
                    io.finished = true;
                    return Ok(());
                }
                self.last_update_ms = now_ms;
            }
            self.input.consume(n_frames * FFT_SIZE);
        }

        if self.input.finished() {
            io.finished = true;
        }

        Ok(())
    }
}

async fn run(
    set_handle: WriteSignal<Option<FlowgraphHandle>, LocalStorage>,
    set_time_data: WriteSignal<Vec<u8>>,
    set_waterfall_data: WriteSignal<Vec<u8>>,
    set_seify_block_id: WriteSignal<Option<usize>>,
) -> Result<()> {
    AsyncRegistry::default()
        .request_permission("")
        .await
        .map_err(|error| {
            futuresdr::runtime::Error::RuntimeError(format!(
                "requesting WebUSB permission for async Seify device: {error}"
            ))
        })?;

    let mut fg = Flowgraph::new();

    let local = fg.local_domain()?;
    let seify_block_id = fg
        .with_local_domain_async(local, async move |ctx: &LocalDomainContext<'_>| {
            let src = AsyncBuilder::new("")
                .await
                .map_err(|error| {
                    futuresdr::runtime::Error::RuntimeError(format!(
                        "opening async Seify device: {error}"
                    ))
                })?
                .frequency(100e6)
                .sample_rate(10e6)
                .build_source_in(ctx)
                .await
                .map_err(|error| {
                    futuresdr::runtime::Error::RuntimeError(format!(
                        "configuring async Seify source: {error}"
                    ))
                })?;
            let seify_block_id = src.id().0;
            let fft = ctx.add(Fft::with_options(
                FFT_SIZE,
                FftDirection::Forward,
                true,
                None,
            ));
            let mag_sqr = ctx.add(Apply::new(|x: &Complex32| x.norm_sqr()));
            let keep = ctx.add(MovingAvg::<FFT_SIZE>::new(0.1, 3));
            let snk = ctx.add(Sink::new(set_time_data, set_waterfall_data));

            connect_async!(ctx, src.outputs[0] ~> fft ~> mag_sqr ~> keep ~> snk);

            Ok(seify_block_id)
        })
        .await?;
    let _ = set_seify_block_id.try_set(Some(seify_block_id));

    let rt = Runtime::new();
    let running = rt.start_async(fg).await.map_err(|error| {
        futuresdr::runtime::Error::RuntimeError(format!("initializing async Seify source: {error}"))
    })?;
    let _ = set_handle.try_set(Some(running.handle()));

    let _ = running.wait_async().await;

    Ok(())
}
