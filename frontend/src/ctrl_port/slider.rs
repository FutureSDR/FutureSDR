use reqwasm::http::Request;
use yew::prelude::*;

use futuresdr_pmt::Pmt;
use futuresdr_pmt::PmtKind;

pub enum Msg {
    Error,
    ValueChanged(i64),
    Reply(String, u64),
}

#[derive(Clone, Properties, PartialEq)]
pub struct Props {
    pub url: String,
    pub block: u32,
    pub callback: u32,
    pub pmt_type: PmtKind,
    pub min: i64,
    pub max: i64,
    pub step: i64,
}

pub struct Slider {
    value: i64,
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
        ctx.link().send_future(async move {
            let response = Request::post(&Self::endpoint(ctx.props()))
                .header("Content-Type", "application/json")
                .body(Json(p))
                .send()
                .await;

            if let Ok(response) = response {
                if response.ok() {
                    Msg::Reply(response.into_body().unwrap(), id)
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
            PmtKind::Double => Some(Pmt::Double(value as f64)),
            _ => None,
        }
    }
}

impl Component for Slider {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let mut status = "<fetching>".to_string();
        let value = ctx.props().min;

        if let Some(p) = Self::value_to_pmt(value, ctx) {
            Self::callback(ctx, &p, 1);
        } else {
            status = "Invalid Properties".to_string();
        }

        Self {
            value,
            status,
            request_id: 1,
            last_request_id: 0,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::ValueChanged(v) => {
                self.status = "<calling>".to_string();

                if let ChangeData::Value(s) = v {
                    if let Ok(v) = s.parse::<i64>() {
                        self.value = v;

                        if let Some(p) = Self::value_to_pmt(self.value, ctx) {
                            self.request_id += 1;
                            Self::callback(ctx, &p, self.request_id);
                        }
                    }
                }

                self.status = "Error".to_string();
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
        html! {
            <div>
                <input type="range"
                value=self.value.to_string()
                min=self.props.min.to_string()
                max=self.props.max.to_string()
                step=self.props.step.to_string()
                onchange=ctx.link().callback(Msg::ValueChanged)
                />
                <span class={classes}>{ &self.result }</span>
            </div>
        }
    }
}
