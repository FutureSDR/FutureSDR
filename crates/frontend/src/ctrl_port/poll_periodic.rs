use gloo_timers::future::sleep;
use reqwasm::http::Request;
use std::time::Duration;
use yew::prelude::*;

use futuresdr_types::Pmt;

pub enum Msg {
    Timeout,
    Error,
    Reply(String),
}

#[derive(Clone, Properties, Default, PartialEq)]
pub struct Props {
    pub url: String,
    pub block: u64,
    pub callback: u64,
    pub interval_secs: f32,
}

pub struct PollPeriodic {
    status: String,
}

impl PollPeriodic {
    fn endpoint(props: &Props) -> String {
        format!(
            "{}/api/block/{}/call/{}/",
            props.url, props.block, props.callback
        )
    }

    fn callback(ctx: &Context<Self>) {
        let endpoint = Self::endpoint(ctx.props());
        gloo_console::log!("poll periodic: sending request");

        ctx.link().send_future(async move {
            let response = Request::post(&endpoint)
                .header("Content-Type", "application/json")
                .body(serde_json::to_string(&Pmt::Null).unwrap())
                .send()
                .await;

            if let Ok(response) = response {
                if response.ok() {
                    return Msg::Reply(response.text().await.unwrap());
                }
            }

            Msg::Error
        });
    }
}

impl Component for PollPeriodic {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let secs = ctx.props().interval_secs;
        ctx.link().send_future(async move {
            sleep(Duration::from_secs_f32(secs)).await;
            Msg::Timeout
        });

        Self {
            status: "init".to_string(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Timeout => {
                Self::callback(ctx);
                self.status = "fetching".to_string();
            }
            Msg::Error => {
                self.status = "Error".to_string();
            }
            Msg::Reply(s) => {
                self.status = s;

                let secs = ctx.props().interval_secs;
                ctx.link().send_future(async move {
                    sleep(Duration::from_secs_f32(secs)).await;
                    Msg::Timeout
                });
            }
        };
        true
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        html! {
            <span>{ &self.status }</span>
        }
    }
}
