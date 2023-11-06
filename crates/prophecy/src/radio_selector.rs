use futuresdr_types::Pmt;
use futuresdr_types::PortId;
use indexmap::IndexMap;
use leptos::logging::*;
use leptos::*;
use uuid::Uuid;

use crate::FlowgraphHandle;

#[component]
pub fn RadioSelector<P: Into<PortId>, V: IntoIterator<Item = (String, Pmt)>>(
    fg_handle: FlowgraphHandle,
    block_id: usize,
    handler: P,
    values: V,
    #[prop(into, optional)] label_class: String,
) -> impl IntoView {
    let handler = handler.into();
    let uuid = Uuid::new_v4();
    let values: IndexMap<String, Pmt> = IndexMap::from_iter(values);

    view! {
        <div>
        { values.into_iter()
            .map(|(n, p)| {
                let fg_handle = fg_handle.clone();
                let handler = handler.clone();
                let label_class = label_class.clone();
                let id = Uuid::new_v4();
                view! {
                    <input type="radio" id={id.to_string()} name={uuid.to_string()} on:change=move |_| {
                        let p = p.clone();
                        let mut fg_handle = fg_handle.clone();
                        let handler = handler.clone();
                        leptos::spawn_local(async move {
                            log!(
                                "sending block {} handler {:?} pmt {:?}",
                                block_id,
                                &handler,
                                &p
                            );
                            let _ = fg_handle
                                .call(block_id, handler, p)
                                .await;
                        });

                    } />
                    <label class={label_class} for={id.to_string()}>{n}</label>
                }
            })
        .collect::<Vec<_>>()
        }
        </div>
    }
}
