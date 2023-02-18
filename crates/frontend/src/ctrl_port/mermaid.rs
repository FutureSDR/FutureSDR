use wasm_bindgen::prelude::*;
use yew::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = "mermaid.init")]
    pub fn init();
}

#[derive(Clone, Properties, Default, PartialEq, Eq)]
pub struct Props {
    pub code: String,
}

pub struct Mermaid {}

impl Component for Mermaid {
    type Message = ();
    type Properties = Props;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {}
    }

    fn rendered(&mut self, _ctx: &Context<Self>, _first_render: bool) {
        init();
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let t = gloo_utils::document().create_element("div").unwrap();
        t.set_class_name("mermaid");
        t.set_inner_html(&ctx.props().code);
        Html::VRef(t.get_root_node())
    }
}
