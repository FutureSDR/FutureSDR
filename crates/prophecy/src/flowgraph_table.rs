use futuresdr_types::FlowgraphDescription;
use leptos::prelude::*;

/// Table representation of blocks in a flowgraph
#[component]
pub fn FlowgraphTable(
    fg: FlowgraphDescription,
    on_message_input_click: Callback<(usize, String, String)>,
) -> impl IntoView {
    if fg.blocks.is_empty() {
        return view! {
            <div class="m-4">
                <p class="text-slate-400 text-sm">"No flowgraph data available."</p>
            </div>
        }
        .into_any();
    }

    let rows: Vec<_> = fg
        .blocks
        .into_iter()
        .map(|block| {
            let block_id = block.id.0;
            let instance_name = block.instance_name.clone();
            let blocking = block.blocking;
            let stream_in = block.stream_inputs.join(", ");
            let stream_out = block.stream_outputs.join(", ");
            let msg_out = block.message_outputs.join(", ");
            view! {
                <tr class="border-b border-slate-700 odd:bg-slate-800 even:bg-slate-700 hover:bg-slate-600 transition-colors">
                    <td class="px-3 py-2 text-slate-400 tabular-nums">{block_id}</td>
                    <td class="px-3 py-2 text-slate-200 font-medium whitespace-nowrap">
                        {instance_name.clone()}
                    </td>
                    <td class="px-3 py-2 text-slate-400 text-xs font-mono whitespace-nowrap">
                        {block.type_name}
                    </td>
                    <td class="px-3 py-2">
                        {if blocking {
                            view! {
                                <span class="bg-violet-600 text-white text-xs px-1.5 py-0.5 rounded">
                                    "yes"
                                </span>
                            }
                            .into_any()
                        } else {
                            view! { <span class="text-slate-500 text-xs">"no"</span> }.into_any()
                        }}
                    </td>
                    <td class="px-3 py-2 text-blue-400 text-xs">{stream_in}</td>
                    <td class="px-3 py-2 text-blue-400 text-xs">{stream_out}</td>
                    <td class="px-3 py-2 text-amber-400 text-xs">
                        <div class="flex flex-wrap gap-1">
                            {block
                                .message_inputs
                                .into_iter()
                                .map(|handler| {
                                    let click_handler = handler.clone();
                                    let instance_name = instance_name.clone();
                                    view! {
                                        <button
                                            class="text-xs px-2 py-0.5 rounded transition-colors"
                                            style="background: #f59e0b; color: #78350f; border: none;"
                                            on:click=move |_| on_message_input_click.run((
                                                block_id,
                                                instance_name.clone(),
                                                click_handler.clone(),
                                            ))
                                        >
                                            {handler}
                                        </button>
                                    }
                                })
                                .collect::<Vec<_>>()}
                        </div>
                    </td>
                    <td class="px-3 py-2 text-amber-400 text-xs">{msg_out}</td>
                </tr>
            }
        })
        .collect();

    view! {
        <div class="m-4">
            <h2 class="text-white text-lg font-semibold mb-3">"Flowgraph Blocks"</h2>
            <div class="overflow-x-auto rounded-lg border border-slate-700">
                <table class="w-full text-sm text-left">
                    <thead class="text-xs text-slate-400 uppercase bg-slate-900 border-b border-slate-700">
                        <tr>
                            <th class="px-3 py-2 whitespace-nowrap">"Block ID"</th>
                            <th class="px-3 py-2 whitespace-nowrap">"Instance Name"</th>
                            <th class="px-3 py-2 whitespace-nowrap">"Type"</th>
                            <th class="px-3 py-2 whitespace-nowrap">"Blocking"</th>
                            <th class="px-3 py-2 whitespace-nowrap">"Stream Inputs"</th>
                            <th class="px-3 py-2 whitespace-nowrap">"Stream Outputs"</th>
                            <th class="px-3 py-2 whitespace-nowrap">"Msg Inputs"</th>
                            <th class="px-3 py-2 whitespace-nowrap">"Msg Outputs"</th>
                        </tr>
                    </thead>
                    <tbody>{rows}</tbody>
                </table>
            </div>
        </div>
    }
    .into_any()
}
