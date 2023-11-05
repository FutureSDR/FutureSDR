use futuresdr::runtime::FlowgraphDescription;
use leptos::*;

#[component]
pub fn FlowgraphCanvas(fg: FlowgraphDescription) -> impl IntoView {
    view! {
        <div> {
            fg.blocks.into_iter()
            .map(|b| {
                let has_stream_inputs = !b.stream_inputs.is_empty();
                let has_stream_outputs = !b.stream_outputs.is_empty();
                let has_message_inputs = !b.message_inputs.is_empty();
                let has_message_outputs = !b.message_outputs.is_empty();
                view! {
                <div>
                    <div class="rounded-full bg-slate-600"> {
                        b.instance_name
                    } </div>
                    <div class="bg-slate-100">
                        <Show
                            when=move || has_stream_inputs
                            fallback=|| ()>
                            <p>"Stream Inputs"</p>
                            {
                                b.stream_inputs.iter()
                                .map(|x| view! {
                                    <p> {x} </p>
                                }).collect::<Vec<_>>()
                            }
                        </Show>
                        <Show
                            when=move || has_stream_outputs
                            fallback=|| ()>
                            <p>"Stream Outputs"</p>
                            {
                                b.stream_outputs.iter()
                                .map(|x| view! {
                                    <p> {x} </p>
                                }).collect::<Vec<_>>()
                            }
                        </Show>
                        <Show
                            when=move || has_message_inputs
                            fallback=|| ()>
                            <p>"Message Inputs"</p>
                            {
                                b.message_inputs.iter()
                                .map(|x| view! {
                                    <p> {x} </p>
                                }).collect::<Vec<_>>()
                            }
                        </Show>
                        <Show
                            when=move || has_message_outputs
                            fallback=|| ()>
                            <p>"Message Outputs"</p>
                            {
                                b.message_outputs.iter()
                                .map(|x| view! {
                                    <p> {x} </p>
                                }).collect::<Vec<_>>()
                            }
                        </Show>
                    </div>
                </div>
            }}).collect::<Vec<_>>()
        } </div>
    }
}
