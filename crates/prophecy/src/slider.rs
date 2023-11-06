use futuresdr_types::Pmt;
use futuresdr_types::PortId;
use leptos::wasm_bindgen::JsCast;
use leptos::*;
use web_sys::HtmlInputElement;

use crate::FlowgraphHandle;

#[component]
pub fn Slider<P: Into<PortId>>(
    fg_handle: FlowgraphHandle,
    block_id: usize,
    handler: P,
    #[prop(default = 0.0)] min: f64,
    #[prop(default = 100.0)] max: f64,
    #[prop(default = 1.0)] step: f64,
    #[prop(optional)] init: Option<f64>,
    #[prop(optional)] setter: Option<WriteSignal<f64>>,
    #[prop(into, optional)] input_class: String,
) -> impl IntoView {
    let handler = handler.into();
    let init = init.unwrap_or(min);

    view! {
        <input type="range" min=min max =max step=step value=init class=input_class
            on:change={
                let handler = handler.clone();
                let fg_handle = fg_handle.clone();

                move |v| {
                    let handler = handler.clone();
                    let mut fg_handle = fg_handle.clone();
                    let target = v.target().unwrap();
                    let input: HtmlInputElement = target.dyn_into().unwrap();
                    let value: f64 = input.value().parse().unwrap();
                    let pmt = Pmt::F64(value);

                    if let Some(setter) = setter {
                        setter(value);
                    }

                    spawn_local(async move {
                        let _ = fg_handle.call(block_id, handler, pmt).await;
                    });
                }
        } />
    }
}
