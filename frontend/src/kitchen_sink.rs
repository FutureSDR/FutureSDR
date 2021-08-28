use futuresdr_pmt::Pmt;
use futuresdr_pmt::PmtKind;
use yew::{html, Component, ComponentLink, Html, ShouldRender};

use crate::ctrl_port::Call;
use crate::ctrl_port::Radio;
use crate::ctrl_port::RadioItem;

pub struct Model;

impl Component for Model {
    type Message = ();
    type Properties = ();

    fn create(_props: Self::Properties, _link: ComponentLink<Self>) -> Self {
        Self {}
    }

    fn update(&mut self, _msg: Self::Message) -> ShouldRender {
        false
    }

    fn change(&mut self, _props: Self::Properties) -> ShouldRender {
        false
    }

    fn view(&self) -> Html {
        html! {
            <div>
                <Call url="http://localhost:1337" block=0 callback=0 pmt_type=PmtKind::U32/>
                // <Poll url="http://localhost:1337" block=0 callback=0 />
                // <PollPeriodic url="http://localhost:1337" block=0 callback=0 interval=1.0 />
                <Radio url="http://localhost:1337" block=0 callback=0 name="hi">
                    <RadioItem id="100" value=Pmt::U32(100_000_000) />
                    <RadioItem id="811" value=Pmt::U32(811_000_000) />
                </Radio>
            </div>
        }
    }
}
