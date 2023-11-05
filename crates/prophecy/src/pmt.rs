use futuresdr::runtime::Pmt;
use leptos::*;

#[component]
pub fn Pmt(#[prop(into)] pmt: MaybeSignal<Pmt>) -> impl IntoView {
    let class = {
        let pmt = pmt.clone();
        move || match pmt() {
            Pmt::Ok => "pmt-ok",
            Pmt::InvalidValue => "pmt-invalidvalue",
            Pmt::Null => "pmt-null",
            Pmt::String(_) => "pmt-string",
            Pmt::Bool(_) => "pmt-bool",
            Pmt::Usize(_) => "pmt-usize",
            Pmt::U32(_) => "pmt-u32",
            Pmt::U64(_) => "pmt-u64",
            Pmt::F32(_) => "pmt-f32",
            Pmt::F64(_) => "pmt-f64",
            Pmt::VecF32(_) => "pmt-vecf32",
            Pmt::VecU64(_) => "pmt-vecu64",
            Pmt::Blob(_) => "pmt-blob",
            Pmt::VecPmt(_) => "pmt-vecpmt",
            Pmt::Finished => "pmt-finished",
            Pmt::MapStrPmt(_) => "pmt-mapstrpmt",
            Pmt::Any(_) => "pmt-any",
            _ => "",
        }
    };

    view! {
        <span class=class>{ move || pmt.get().to_string() }</span>
    }
}
