use reqwasm::http::Request;
use web_sys::HtmlInputElement;
use yew::prelude::*;

use futuresdr_pmt::Pmt;
use futuresdr_pmt::PmtKind;

pub enum Msg {
    Error,
    Reply(String),
    Submit(String),
}

#[derive(Clone, Properties, PartialEq, Eq)]
pub struct Props {
    pub url: String,
    pub block: u64,
    pub callback: u64,
    pub pmt_type: PmtKind,
}

pub struct Call {
    status: String,
}

impl Call {
    fn endpoint(props: &Props) -> String {
        format!(
            "{}/api/block/{}/call/{}",
            props.url, props.block, props.callback
        )
    }

    fn callback(ctx: &Context<Self>, p: &Pmt) {
        let p = p.clone();
        let endpoint = Self::endpoint(ctx.props());
        gloo_console::log!(format!("call: sending request {:?}", &p));

        ctx.link().send_future(async move {
            let response = Request::post(&endpoint)
                .header("Content-Type", "application/json")
                .body(serde_json::to_string(&p).unwrap())
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

impl Component for Call {
    type Message = Msg;
    type Properties = Props;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            status: "init".to_string(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Submit(s) => {
                if let Some(p) = Pmt::from_string(&s, &ctx.props().pmt_type) {
                    Self::callback(ctx, &p);
                } else {
                    self.status = "Parse Error".to_string();
                }
            }
            Msg::Error => {
                self.status = "Error".to_string();
            }
            Msg::Reply(v) => {
                self.status = v;
            }
        };
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let onkeypress = ctx.link().batch_callback(|e: KeyboardEvent| {
            if e.key() == "Enter" {
                let input: HtmlInputElement = e.target_unchecked_into();
                Some(Msg::Submit(input.value()))
            } else {
                None
            }
        });

        html! {
            <div>
                <input class="edit"
                    type="text"
                    { onkeypress }
                />
                <span>{ &self.status }</span>
            </div>
        }
    }
}
