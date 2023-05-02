//! Call a message handler, selecting a PMT through radio buttons
use futuresdr_types::Pmt;
use reqwasm::http::Request;
use std::rc::Rc;
use yew::prelude::*;

#[doc(hidden)]
#[derive(Clone, Properties, PartialEq)]
pub struct RadioItemProps {
    pub value: Pmt,
    #[prop_or(false)]
    pub checked: bool,
}

/// A Radio button for a PMT
#[derive(Clone)]
pub struct RadioItem;

impl Component for RadioItem {
    type Message = ();
    type Properties = RadioItemProps;

    fn create(_ctx: &Context<Self>) -> Self {
        Self
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let parent = ctx.link().get_parent().unwrap().clone();
        let radio = parent.downcast::<Radio>();
        let p = ctx.props().value.clone();
        let onclick = radio.callback(move |_| Msg::Value(p.clone()));

        html! {
            <>
                <input type="radio" {onclick} checked={ctx.props().checked}/>
                <label>{format!("{:?}", &ctx.props().value)}</label>
            </>
        }
    }
}

#[doc(hidden)]
#[derive(Clone, Properties, PartialEq)]
pub struct Props {
    pub children: ChildrenWithProps<RadioItem>,
    pub url: String,
    pub block: u64,
    pub callback: u64,
}

/// Call a message handler, selecting a PMT through radio buttons
pub struct Radio {
    value: Pmt,
    status: String,
}

#[doc(hidden)]
pub enum Msg {
    Submit,
    Value(Pmt),
    Reply(String),
    Error,
}

impl Radio {
    fn endpoint(props: &Props) -> String {
        format!(
            "{}/api/block/{}/call/{}/",
            props.url, props.block, props.callback
        )
    }

    fn callback(ctx: &Context<Self>, p: &Pmt) {
        let p = p.clone();
        let endpoint = Self::endpoint(ctx.props());
        gloo_console::log!(format!("radio: sending request {:?}", &p));

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

impl Component for Radio {
    type Message = Msg;
    type Properties = Props;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            status: "Init".to_string(),
            value: Pmt::Null,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Submit => {
                self.status = "Fetching".to_string();
                Self::callback(ctx, &self.value);
            }
            Msg::Value(p) => {
                self.value = p;
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
        let onclick = ctx.link().callback(|_| Msg::Submit);

        html! {
            <>
            {
                for ctx.props().children.iter().map(|mut item| {
                    let props = Rc::make_mut(&mut item.props);
                    if props.value == self.value {
                        props.checked = true;
                    } else {
                        props.checked = false;
                    }
                    item
                })
            }

                <button type="submit" {onclick}>{"Submit"}</button>
                <div>{&self.status}</div>
            </>
        }
    }
}
