use futuresdr_types::Pmt;
use futuresdr_types::PortId;
use indexmap::IndexMap;
use leptos::html::Select;
use leptos::logging::*;
use leptos::*;

use crate::FlowgraphHandle;

#[component]
/// List Selector
///
/// Selecting an entry from a list triggers sending a PMT.
pub fn ListSelector<P: Into<PortId>, V: IntoIterator<Item = (String, Pmt)>>(
    fg_handle: FlowgraphHandle,
    block_id: usize,
    handler: P,
    values: V,
    #[prop(into, optional)] select_class: String,
) -> impl IntoView {
    let handler = handler.into();
    let select_ref = create_node_ref::<Select>();
    let values: IndexMap<String, Pmt> = IndexMap::from_iter(values);

    let change = {
        let values = values.clone();
        move |_| {
            let mut fg_handle = fg_handle.clone();
            let handler = handler.clone();
            let select = select_ref.get().unwrap();
            let pmt = values.get(&select.value()).unwrap().clone();
            leptos::spawn_local(async move {
                log!(
                    "sending block {} handler {:?} pmt {:?}",
                    block_id,
                    &handler,
                    &pmt
                );
                let _ = fg_handle.call(block_id, handler, pmt).await;
            });
        }
    };

    view! {
        <select node_ref=select_ref on:change=change class={select_class}> {
            values.into_iter()
            .map(|(n, _)| view! {
                <option value={n.clone()}>{n}</option>
            })
            .collect::<Vec<_>>()
        }
        </select>
    }
}
