use reqwasm::http::Request;
use yew::prelude::*;

use futuresdr_pmt::Pmt;

pub enum Msg {
    Poll,
    Error,
    Reply(String),
}

#[derive(Clone, Properties, Default, PartialEq, Eq)]
pub struct Props {
    pub url: String,
    pub block: u64,
    pub callback: u64,
}

pub struct Poll {
    status: String,
}

impl Poll {
    fn endpoint(props: &Props) -> String {
        format!(
            "{}/api/block/{}/call/{}/",
            props.url, props.block, props.callback
        )
    }

    fn callback(ctx: &Context<Self>) {
        let endpoint = Self::endpoint(ctx.props());
        gloo_console::log!("poll: sending request");

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

impl Component for Poll {
    type Message = Msg;
    type Properties = Props;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            status: "init".to_string(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Poll => {
                Self::callback(ctx);
            }
            Msg::Error => {
                self.status = "Error".to_string();
            }
            Msg::Reply(s) => {
                self.status = s;
            }
        };
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let onclick = ctx.link().callback(|_| Msg::Poll);

        html! {
            <div>
                <button { onclick }>{ "Update" }</button>
                <span>{ &self.status }</span>
            </div>
        }
    }
}
