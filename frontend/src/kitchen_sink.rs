// use futuresdr_pmt::Pmt;
use futuresdr_pmt::PmtKind;
use yew::prelude::*;

use crate::ctrl_port::call::Call;
use crate::ctrl_port::poll::Poll;
use crate::ctrl_port::poll_periodic::PollPeriodic;
use crate::ctrl_port::slider::Slider;
// use crate::ctrl_port::Radio;
// use crate::ctrl_port::RadioItem;

#[function_component(KitchenSink)]
pub fn kitchen_sink() -> Html {
    html! {
        <div>
            <Call url="http://localhost:1337" block=0 callback=0 pmt_type={PmtKind::U32}/>
            <Poll url="http://localhost:1337" block=0 callback=0/>
            <PollPeriodic url="http://localhost:1337" block=0 callback=0 interval_secs=0.8/>
            <Slider url="http://localhost:1337" block=0 callback=0 pmt_type={PmtKind::U32} min=0 max=100 step=1 value=30/>
            // <Radio url="http://localhost:1337" block=0 callback=0 name="hi">
            //     <RadioItem id="100" value=Pmt::U32(100_000_000) />
            //     <RadioItem id="811" value=Pmt::U32(811_000_000) />
            // </Radio>
        </div>
    }
}
