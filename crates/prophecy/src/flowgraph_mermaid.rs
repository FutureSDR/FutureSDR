use futuresdr_types::FlowgraphDescription;
use leptos::html::div;
use leptos::html::pre;
use leptos::*;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = "mermaid.init")]
    pub fn mermaid_render();
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
        let src_port = &fg.blocks[e.0].stream_outputs[e.1];
        let dst_port = &fg.blocks[e.2].stream_inputs[e.3];
        let con = src_port.clone() + " > " + &dst_port;
        g.push_str(&format!("N{}-->|{}| N{};\n", e.0, con, e.2));
    }
    for e in fg.message_edges {
        let src_port = &fg.blocks[e.0].message_outputs[e.1];
        let dst_port = &fg.blocks[e.2].message_inputs[e.3];
        let con = src_port.clone() + " > " + &dst_port;
        g.push_str(&format!("N{}-.->|{}| N{};\n", e.0, con, e.2));
    }
    g
}

#[component]
pub fn FlowgraphMermaid(fg: FlowgraphDescription) -> impl IntoView {
    div().on_mount(|_| mermaid_render()).child(
        pre()
            .classes("mermaid")
            .inner_html(flowgraph_to_mermaid(fg)),
    )
}
