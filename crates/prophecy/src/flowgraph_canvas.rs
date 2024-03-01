use leptos::html::Div;
use leptos::*;
use leptos_use::core::Position;
use leptos_use::*;

#[component]
/// Flowgraph Canvas (WIP)
pub fn FlowgraphCanvas() -> impl IntoView {
    let el = create_node_ref::<Div>();
    let UseDraggableReturn { style, .. } = use_draggable_with_options(
        el,
        UseDraggableOptions::default().initial_value(Position { x: 40.0, y: 40.0 }),
    );
    view! {
        <div class="bg-red-500" style="width=100%; height: 50vh" >
            <div node_ref=el class="select-none cursor-move text-white bg-blue-500 p-5" style=move || format!("position: absolute; {}", style.get())>
            foo
            </div>
        </div>
    }
}

// #[component]
// pub fn FlowgraphCanvas(fg: FlowgraphDescription) -> impl IntoView {
//     view! {
//         <div> {
//             fg.blocks.into_iter()
//             .map(|b| {
//                 let has_stream_inputs = !b.stream_inputs.is_empty();
//                 let has_stream_outputs = !b.stream_outputs.is_empty();
//                 let has_message_inputs = !b.message_inputs.is_empty();
//                 let has_message_outputs = !b.message_outputs.is_empty();
//                 view! {
//                 <div>
//                     <div class="rounded-full bg-slate-600"> {
//                         b.instance_name
//                     } </div>
//                     <div class="bg-slate-100">
//                         <Show
//                             when=move || has_stream_inputs
//                             fallback=|| ()>
//                             <p>"Stream Inputs"</p>
//                             {
//                                 b.stream_inputs.iter()
//                                 .map(|x| view! {
//                                     <p> {x} </p>
//                                 }).collect::<Vec<_>>()
//                             }
//                         </Show>
//                         <Show
//                             when=move || has_stream_outputs
//                             fallback=|| ()>
//                             <p>"Stream Outputs"</p>
//                             {
//                                 b.stream_outputs.iter()
//                                 .map(|x| view! {
//                                     <p> {x} </p>
//                                 }).collect::<Vec<_>>()
//                             }
//                         </Show>
//                         <Show
//                             when=move || has_message_inputs
//                             fallback=|| ()>
//                             <p>"Message Inputs"</p>
//                             {
//                                 b.message_inputs.iter()
//                                 .map(|x| view! {
//                                     <p> {x} </p>
//                                 }).collect::<Vec<_>>()
//                             }
//                         </Show>
//                         <Show
//                             when=move || has_message_outputs
//                             fallback=|| ()>
//                             <p>"Message Outputs"</p>
//                             {
//                                 b.message_outputs.iter()
//                                 .map(|x| view! {
//                                     <p> {x} </p>
//                                 }).collect::<Vec<_>>()
//                             }
//                         </Show>
//                     </div>
//                 </div>
//             }}).collect::<Vec<_>>()
//         } </div>
//     }
// }
