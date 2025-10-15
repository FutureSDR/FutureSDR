use futuresdr_types::FlowgraphDescription;
use leptos::html::Pre;
use leptos::prelude::*;
use leptos::wasm_bindgen;
use leptos::wasm_bindgen::prelude::*;

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
            b.id.0, b.type_name, b.instance_name, b.blocking
        ));
    }

    for e in fg.stream_edges {
        let src_port = e.1.name();
        let dst_port = e.3.name();
        let con = "\"".to_string() + src_port + " > " + dst_port + "\"";
        g.push_str(&format!("N{}-->|{}| N{};\n", e.0.0, con, e.2.0));
    }
    for e in fg.message_edges {
        let src_port = e.1.name();
        let dst_port = e.3.name();
        let con = src_port.to_string() + " > " + dst_port;
        g.push_str(&format!("N{}-.->|{}| N{};\n", e.0.0, con, e.2.0));
    }
    g
}

#[component]
/// Mermaid Graph of Flowgraph
pub fn FlowgraphMermaid(fg: FlowgraphDescription) -> impl IntoView {
    let pre_ref = NodeRef::<Pre>::new();

    Effect::new(move |_| {
        if let Some(pre) = pre_ref.get() {
            pre.set_inner_html(&flowgraph_to_mermaid(fg.clone()));
            mermaid_render();
        }
    });

    view! {
        <div>
            <pre class="mermaid" node_ref=pre_ref></pre>
        </div>
    }
}
