use futuresdr_pmt::Pmt;
use futuresdr_pmt::PmtKind;
use wasm_bindgen::prelude::*;
use yew::prelude::*;

use crate::ctrl_port::call::Call;
use crate::ctrl_port::poll::Poll;
use crate::ctrl_port::poll_periodic::PollPeriodic;
use crate::ctrl_port::radio::Radio;
use crate::ctrl_port::radio::RadioItem;
use crate::ctrl_port::slider::Slider;

#[wasm_bindgen]
pub fn kitchen_sink(id: String) {
    let document = gloo_utils::document();
    let div = document.query_selector(&id).unwrap().unwrap();
    yew::start_app_in_element::<KitchenSink>(div);
}

#[function_component(KitchenSink)]
pub fn kitchen_sink() -> Html {
    html! {
        <div>
            <Call url="http://127.0.0.1:1337" block=0 callback=0 pmt_type={PmtKind::U32}/>
            <Poll url="http://127.0.0.1:1337" block=0 callback=0/>
            <PollPeriodic url="http://127.0.0.1:1337" block=0 callback=0 interval_secs=3.8/>
            <Slider url="http://127.0.0.1:1337" block=0 callback=0 pmt_type={PmtKind::U32} min=0 max=100 step=1 value=30/>
            <Radio url="http://127.0.0.1:1337" block=0 callback=0>
                <RadioItem value={Pmt::U32(100_000_000)}/>
                <RadioItem value={Pmt::U32(811_000_000)}/>
            </Radio>
        </div>
    }
}
