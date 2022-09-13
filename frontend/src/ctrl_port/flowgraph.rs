use reqwasm::http::Request;
use wasm_bindgen::prelude::*;
use yew::prelude::*;

use futuresdr_pmt::FlowgraphDescription;

use crate::ctrl_port::mermaid::Mermaid;

#[wasm_bindgen]
pub fn add_flowgraph(id: String, url: String) {
    let document = gloo_utils::document();
    let div = document.query_selector(&id).unwrap().unwrap();
    yew::start_app_with_props_in_element::<Flowgraph>(div, Props { url });
}

pub enum Msg {
    Error,
    Reply(FlowgraphDescription),
}

#[derive(Clone, Properties, Default, PartialEq, Eq)]
pub struct Props {
    pub url: String,
}

pub struct Flowgraph {
    code: String,
}

impl Flowgraph {
    fn endpoint(props: &Props) -> String {
        format!("{}/api/fg/", props.url)
    }

    fn callback(ctx: &Context<Self>) {
        let endpoint = Self::endpoint(ctx.props());
        gloo_console::log!("flowgraph: sending request");

        ctx.link().send_future(async move {
            let response = Request::get(&endpoint).send().await;

            if let Ok(response) = response {
                if let Ok(fg) = response.json().await {
                    return Msg::Reply(fg);
                }
            }

            Msg::Error
        });
    }

    fn flowgraph_to_mermaid(fg: FlowgraphDescription) -> String {
        let mut g = String::from("graph LR;\n");

        for b in fg.blocks.iter() {
            g.push_str(&format!(
                "N{}[{}<br/><b>name:</b>{}<br/><b>is blocking</b>:{}];\n",
                b.id, b.type_name, b.instance_name, b.blocking
            ));
        }
        for e in fg.stream_edges {
            g.push_str(&format!("N{}-->N{};\n", e.0, e.2));
        }
        for e in fg.message_edges {
            g.push_str(&format!("N{}-.->N{};\n", e.0, e.2));
        }
        g
    }
}

impl Component for Flowgraph {
    type Message = Msg;
    type Properties = Props;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            code: "flowchart LR".to_string(),
        }
    }

    fn rendered(&mut self, ctx: &Context<Self>, first_render: bool) {
        if first_render {
            Self::callback(ctx);
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Error => {
                self.code = r#"flowchart LR
                                  id1(Error)
                                  style id1 color:#000,fill:#f00,stroke:#000,stroke-width:4px
                            "#
                .to_string();
            }
            Msg::Reply(fg) => {
                self.code = Self::flowgraph_to_mermaid(fg);
            }
        };
        true
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        html! {
            <Mermaid code={self.code.clone()} />
        }
    }
}
