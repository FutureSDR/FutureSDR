use futuresdr::runtime::Pmt;
use futuresdr::runtime::PortId;
use leptos::logging::*;
use leptos::*;
use std::collections::HashMap;
use uuid::Uuid;

use crate::FlowgraphHandle;

#[component]
pub fn RadioSelector<P: Into<PortId>>(
    fg_handle: FlowgraphHandle,
    block_id: usize,
    handler: P,
    values: HashMap<String, Pmt>,
) -> impl IntoView {
    let handler = handler.into();
    let uuid = Uuid::new_v4();

    view! {
        <div>
        { values.into_iter()
            .map(|(n, p)| {
                let fg_handle = fg_handle.clone();
                let handler = handler.clone();
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
                    <label for={n.clone()}>{n}</label>
                }
            })
        .collect::<Vec<_>>()
        }
        </div>
    }
}
