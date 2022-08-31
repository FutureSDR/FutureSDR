use reqwasm::http::Request;
use wasm_bindgen::prelude::*;
use web_sys::HtmlInputElement;
use yew::prelude::*;

use futuresdr_pmt::Pmt;
use futuresdr_pmt::PmtKind;

#[allow(clippy::too_many_arguments)]
#[wasm_bindgen]
pub fn add_slider_u32(
    id: String,
    url: String,
    block: u32,
    callback: u32,
    min: f64,
    max: f64,
    step: f64,
    value: f64,
) {
    let document = gloo_utils::document();
    let div = document.query_selector(&id).unwrap().unwrap();
    yew::start_app_with_props_in_element::<Slider>(
        div,
        Props {
            url,
            block,
            callback,
            pmt_type: PmtKind::U32,
            min: min as i64,
            max: max as i64,
            step: step as i64,
            value: value as i64,
        },
    );
}

pub enum Msg {
    Error,
    ValueChanged(i64),
    Reply(String, u64),
}

#[derive(Clone, Properties, PartialEq, Eq)]
pub struct Props {
    pub url: String,
    pub block: u32,
    pub callback: u32,
    pub pmt_type: PmtKind,
    pub min: i64,
    pub max: i64,
    pub step: i64,
    pub value: i64,
}

pub struct Slider {
    status: String,
    request_id: u64,
    last_request_id: u64,
}

impl Slider {
    fn endpoint(props: &Props) -> String {
        format!(
            "{}/api/block/{}/call/{}",
            props.url, props.block, props.callback
        )
    }

    fn callback(ctx: &Context<Self>, p: &Pmt, id: u64) {
        let p = p.clone();
        let endpoint = Self::endpoint(ctx.props());
        gloo_console::log!(format!("slider: sending request {:?}", &p));

        ctx.link().send_future(async move {
            let response = Request::post(&endpoint)
                .header("Content-Type", "application/json")
                .body(serde_json::to_string(&p).unwrap())
                .send()
                .await;

            if let Ok(response) = response {
                if response.ok() {
                    return Msg::Reply(response.text().await.unwrap(), id);
                }
            }
            Msg::Error
        });
    }

    fn value_to_pmt(value: i64, ctx: &Context<Self>) -> Option<Pmt> {
        match ctx.props().pmt_type {
            PmtKind::U32 => {
                let v = u32::try_from(value).ok()?;
                Some(Pmt::U32(v))
            }
            PmtKind::U64 => {
                let v = u64::try_from(value).ok()?;
                Some(Pmt::U64(v))
            }
            PmtKind::F64 => Some(Pmt::F64(value as f64)),
            _ => None,
        }
    }
}

impl Component for Slider {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let mut status = "init".to_string();
        let value = ctx.props().value;

        if let Some(p) = Self::value_to_pmt(value, ctx) {
            Self::callback(ctx, &p, 1);
        } else {
            status = "Invalid Properties".to_string();
        }

        Self {
            status,
            request_id: 1,
            last_request_id: 0,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::ValueChanged(v) => {
                self.status = "calling".to_string();

                if let Some(p) = Self::value_to_pmt(v, ctx) {
                    self.request_id += 1;
                    Self::callback(ctx, &p, self.request_id);
                } else {
                    self.status = "Invalid Value".to_string();
                }
            }
            Msg::Error => {
                self.status = "Error".to_string();
            }
            Msg::Reply(v, req_id) => {
                if req_id > self.last_request_id {
                    self.status = v;
                }
            }
        };
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let mut classes = "".to_string();
        if self.request_id > self.last_request_id {
            classes.push_str(" fetching");
        }

        let oninput = ctx.link().callback(|e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            Msg::ValueChanged(input.value_as_number() as i64)
        });

        html! {
            <div>
                <input type="range"
                    min={ctx.props().min.to_string()}
                    max={ctx.props().max.to_string()}
                    step={ctx.props().step.to_string()}
                    {oninput}
                />

                <span class={classes}>{ &self.status }</span>
            </div>
        }
    }
}
