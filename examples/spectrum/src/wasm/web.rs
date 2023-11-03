use prophecy::leptos::html::Span;
use prophecy::leptos::wasm_bindgen::JsCast;
use prophecy::leptos::*;
use prophecy::FlowgraphMermaid;
use prophecy::ListSelector;
use prophecy::TimeSink;
use prophecy::TimeSinkMode;
use prophecy::Waterfall;
use prophecy::WaterfallMode;
use std::cell::RefCell;
use std::rc::Rc;
use web_sys::HtmlInputElement;

use futuresdr::anyhow::Result;
use futuresdr::blocks::wasm::HackRf;
use futuresdr::blocks::Fft;
use futuresdr::blocks::FftDirection;
use futuresdr::macros::async_trait;
use futuresdr::macros::connect;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::FlowgraphHandle;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::WorkIo;

const FFT_SIZE: usize = 2048;

#[component]
pub fn Spectrum(
    handle: prophecy::FlowgraphHandle,
    time_data: Rc<RefCell<Option<Vec<u8>>>>,
    waterfall_data: Rc<RefCell<Option<Vec<u8>>>>,
) -> impl IntoView {
    let fg_desc = create_local_resource(|| (), {
        let handle = handle.clone();
        move |_| {
            let mut handle = handle.clone();
            async move {
                if let Ok(desc) = handle.description().await {
                    return Some(desc);
                }
                None
            }
        }
    });

    let (min, set_min) = create_signal(-40.0f32);
    let (max, set_max) = create_signal(20.0f32);

    let min_label = create_node_ref::<Span>();
    let max_label = create_node_ref::<Span>();
    let freq_label = create_node_ref::<Span>();

    let (ctrl, set_ctrl) = create_signal(true);
    let ctrl_click = move |_| {
        set_ctrl(!ctrl());
    };

    view! {
        <div class="text-white">
            <button class="bg-slate-600 hover:bg-slate-800 rounded p-2 m-4" on:click=ctrl_click>Show/Hide Controlls</button>
        </div>
        <Show when=ctrl>
            <div class="border-2 border-slate-500 rounded-md flex flex-row flex-wrap m-4 p-4 gap-y-4">
                <div class="basis-1/3">
                    <input type="range" min="-100" max="50" value="-40" class="align-middle"
                        on:change= move |v| {
                            let target = v.target().unwrap();
                            let input : HtmlInputElement = target.dyn_into().unwrap();
                            min_label.get().unwrap().set_inner_text(&format!("min: {} dB", input.value()));
                            set_min(input.value().parse().unwrap());
                        } />
                    <span class="text-white p-2 m-2" node_ref=min_label>"min: -40 dB"</span>
                </div>
                <div class="basis-1/3">
                    <input type="range" min="-40" max="100" value="20" class="align-middle"
                        on:change= move |v| {
                            let target = v.target().unwrap();
                            let input : HtmlInputElement = target.dyn_into().unwrap();
                            max_label.get().unwrap().set_inner_text(&format!("max: {} dB", input.value()));
                            set_max(input.value().parse().unwrap());
                        } />
                    <span class="text-white p-2 m-2" node_ref=max_label>"max: 20 dB"</span>
                </div>
                <div class="basis-1/3">
                    <input type="range" min="100" max="2500" value="100" class="align-middle"
                        on:change= {
                            let handle = handle.clone();
                            move |v| {
                                let target = v.target().unwrap();
                                let input : HtmlInputElement = target.dyn_into().unwrap();
                                freq_label.get().unwrap().set_inner_text(&format!("freq: {} MHz", input.value()));
                                let freq : f64 = input.value().parse().unwrap();
                                let p = Pmt::F64(freq * 1e6);
                                let mut handle = handle.clone();
                                spawn_local(async move {
                                    let _ = handle.call(0, "freq", p).await;
                                });
                    }} />
                    <span class="text-white p-2 m-2" node_ref=freq_label>"freq: 100 MHz"</span>
                </div>
                <div class="basis-1/3">
                    <span class="text-white m-2">Amp</span>
                    <ListSelector fg_handle=handle.clone() block_id=0 handler="amp" values=[
                        ("Disable".to_string(), Pmt::Bool(false)),
                        ("Enable".to_string(), Pmt::Bool(true)),
                    ] />
                </div>
                <div class="basis-1/3">
                    <span class="text-white m-2">LNA Gain</span>
                    <ListSelector fg_handle=handle.clone() block_id=0 handler="lna" values=[
                        ("0".to_string(), Pmt::U32(0)),
                        ("8".to_string(), Pmt::U32(8)),
                        ("16".to_string(), Pmt::U32(16)),
                        ("24".to_string(), Pmt::U32(24)),
                        ("32".to_string(), Pmt::U32(32)),
                        ("40".to_string(), Pmt::U32(40)),
                    ] />
                </div>
                <div class="basis-1/3">
                    <span class="text-white m-2">VGA Gain</span>
                    <ListSelector fg_handle=handle.clone() block_id=0 handler="vga" values=[
                        ("0".to_string(), Pmt::U32(0)),
                        ("8".to_string(), Pmt::U32(8)),
                        ("16".to_string(), Pmt::U32(16)),
                        ("24".to_string(), Pmt::U32(24)),
                        ("32".to_string(), Pmt::U32(32)),
                        ("40".to_string(), Pmt::U32(40)),
                        ("48".to_string(), Pmt::U32(48)),
                        ("56".to_string(), Pmt::U32(56)),
                    ] />
                </div>
                <div class="basis-1/3">
                    <span class="text-white m-2">Sample Rate</span>
                    <ListSelector fg_handle=handle.clone() block_id=0 handler="sample_rate" values=[
                        ("2 MHz".to_string(), Pmt::F64(2e6)),
                        ("4 MHz".to_string(), Pmt::F64(4e6)),
                        ("8 MHz".to_string(), Pmt::F64(8e6)),
                        ("16 MHz".to_string(), Pmt::F64(16e6)),
                        ("20 MHz".to_string(), Pmt::F64(20e6)),
                    ] />
                </div>
            </div>
        </Show>
        <div class="border-2 border-slate-500 rounded-md m-4" style="height: 400px; max-height: 40vh">
            <TimeSink min=min max=max mode=TimeSinkMode::Data(time_data) />
        </div>
        <div class="border-2 border-slate-500 rounded-md m-4" style="height: 400px; max-height: 40vh">
            <Waterfall min=min max=max mode=WaterfallMode::Data(waterfall_data) />
        </div>
        <div class="border-2 border-slate-500 rounded-md m-4 p-4">
            {move || {
                match fg_desc.get() {
                    Some(Some(desc)) => view! { <FlowgraphMermaid fg=desc /> }.into_view(),
                    _ => view! {}.into_view(),
                }
            }}
        </div>
    }
}

#[component]
pub fn Gui() -> impl IntoView {
    let (handle, set_handle) = create_signal(None);

    let data = vec![Rc::new(RefCell::new(None)), Rc::new(RefCell::new(None))];
    let time_data = data[0].clone();
    let waterfall_data = data[1].clone();

    view! {
        <h1 class="text-xl text-white m-4"> FutureSDR Spectrum</h1>
        {
            let time_data = time_data;
            let waterfall_data = waterfall_data;
            move || {
             match handle.get() {
                 Some(handle) => {
                     let handle = prophecy::FlowgraphHandle::from_handle(handle);
                     view! {
                         <Spectrum handle=handle time_data=time_data.clone() waterfall_data=waterfall_data.clone() /> }.into_view()
                 },
                 _ => view! {
                     <div class="text-white m-4 space-y-4">
                         <button class="p-2 rounded bg-slate-600 hover:bg-slate-700" on:click={
                             let data = data.clone();
                             move |_| {
                             leptos::spawn_local({
                                 let data = data.clone();
                                 async move {
                                     run(set_handle, data).await.unwrap();
                                 }});
                         }}>Start</button>
                         <div>"Please Click to Start Flowgraph"</div>
                     </div>
                 }.into_view(),
             }
        }}
    }
}

pub fn web() {
    _ = console_log::init_with_level(log::Level::Debug);
    console_error_panic_hook::set_once();
    mount_to_body(|| view! { <Gui /> })
}

pub struct Sink {
    data: Vec<Rc<RefCell<Option<Vec<u8>>>>>,
}

unsafe impl Send for Sink {}

impl Sink {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(data: Vec<Rc<RefCell<Option<Vec<u8>>>>>) -> Block {
        Block::new(
            BlockMetaBuilder::new("Sink").build(),
            StreamIoBuilder::new().add_input::<f32>("in").build(),
            MessageIoBuilder::new().build(),
            Self { data },
        )
    }
}

#[async_trait]
impl Kernel for Sink {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let input = sio.input(0).slice::<f32>();
        // log!("sink len {} io {:?}", input.len(), &io);

        if input.len() >= 2048 {
            let samples = &input[0..2048];
            let bytes = unsafe {
                let l = samples.len() * 4;
                let p = samples.as_ptr();
                std::slice::from_raw_parts(p as *const u8, l)
            };
            for d in &self.data {
                *d.borrow_mut() = Some(Vec::from(bytes));
            }
            sio.input(0).consume(2048);
        }

        if input.len() >= 2048 * 2 {
            io.call_again = true;
        } else if sio.input(0).finished() {
            io.finished = true;
        }

        Ok(())
    }
}

async fn run(
    set_handle: WriteSignal<Option<FlowgraphHandle>>,
    data: Vec<Rc<RefCell<Option<Vec<u8>>>>>,
) -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = HackRf::new();
    let fft = Fft::with_options(FFT_SIZE, FftDirection::Forward, true, None);
    let mag_sqr = crate::power_block();
    let keep = crate::Keep1InN::<FFT_SIZE>::new(0.1, 3);
    let snk = Sink::new(data);

    futuresdr::runtime::config::set("slab_reserved", 0);
    connect!(fg, src > fft > mag_sqr > keep > snk);

    let rt = Runtime::new();
    let (task, handle) = rt.start(fg).await;
    set_handle.set(Some(handle));

    let _ = task.await;

    Ok(())
}
