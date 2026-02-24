use std::collections::HashMap;

use futuresdr_types::Pmt;
use futuresdr_types::PmtKind;
use leptos::html::Input;
use leptos::html::Select;
use leptos::html::Textarea;
use leptos::prelude::*;

pub const JSON_PMT_TYPES: [PmtKind; 18] = [
    PmtKind::Ok,
    PmtKind::InvalidValue,
    PmtKind::Null,
    PmtKind::String,
    PmtKind::Bool,
    PmtKind::Usize,
    PmtKind::Isize,
    PmtKind::U32,
    PmtKind::U64,
    PmtKind::F32,
    PmtKind::F64,
    PmtKind::VecCF32,
    PmtKind::VecF32,
    PmtKind::VecU64,
    PmtKind::Blob,
    PmtKind::VecPmt,
    PmtKind::MapStrPmt,
    PmtKind::Finished,
];

pub fn parse_json_pmt(kind: PmtKind, input: &str) -> Result<Pmt, String> {
    let v = input.trim();

    match kind {
        PmtKind::Ok => Ok(Pmt::Ok),
        PmtKind::InvalidValue => Ok(Pmt::InvalidValue),
        PmtKind::Null => Ok(Pmt::Null),
        PmtKind::Finished => Ok(Pmt::Finished),
        PmtKind::String => {
            if v.is_empty() {
                Ok(Pmt::String(String::new()))
            } else if let Ok(s) = serde_json::from_str::<String>(v) {
                Ok(Pmt::String(s))
            } else {
                Ok(Pmt::String(v.to_string()))
            }
        }
        PmtKind::Bool => v
            .parse::<bool>()
            .map(Pmt::Bool)
            .map_err(|_| "expected true or false".to_string()),
        PmtKind::U32 => v
            .parse::<u32>()
            .map(Pmt::U32)
            .map_err(|_| "expected u32".to_string()),
        PmtKind::Usize => v
            .parse::<usize>()
            .map(Pmt::Usize)
            .map_err(|_| "expected usize".to_string()),
        PmtKind::Isize => v
            .parse::<isize>()
            .map(Pmt::Isize)
            .map_err(|_| "expected isize".to_string()),
        PmtKind::U64 => v
            .parse::<u64>()
            .map(Pmt::U64)
            .map_err(|_| "expected u64".to_string()),
        PmtKind::F32 => v
            .parse::<f32>()
            .map(Pmt::F32)
            .map_err(|_| "expected f32".to_string()),
        PmtKind::F64 => v
            .parse::<f64>()
            .map(Pmt::F64)
            .map_err(|_| "expected f64".to_string()),
        PmtKind::VecF32 => {
            if let Ok(list) = serde_json::from_str::<Vec<f32>>(v) {
                Ok(Pmt::VecF32(list))
            } else {
                v.parse::<f32>()
                    .map(|n| Pmt::VecF32(vec![n]))
                    .map_err(|_| "expected JSON array like [1.0, 2.0] or one f32".to_string())
            }
        }
        PmtKind::VecU64 => {
            if let Ok(list) = serde_json::from_str::<Vec<u64>>(v) {
                Ok(Pmt::VecU64(list))
            } else {
                v.parse::<u64>()
                    .map(|n| Pmt::VecU64(vec![n]))
                    .map_err(|_| "expected JSON array like [1, 2] or one u64".to_string())
            }
        }
        PmtKind::VecCF32 => serde_json::from_str::<Pmt>(&format!(r#"{{"VecCF32":{v}}}"#))
            .map_err(|e| format!("expected JSON complex array like [{{\"re\":1.0,\"im\":2.0}}]: {e}")),
        PmtKind::Blob => {
            if let Ok(list) = serde_json::from_str::<Vec<u8>>(v) {
                Ok(Pmt::Blob(list))
            } else {
                v.parse::<u8>()
                    .map(|n| Pmt::Blob(vec![n]))
                    .map_err(|_| "expected JSON byte array like [1, 2, 255]".to_string())
            }
        }
        PmtKind::VecPmt => serde_json::from_str::<Vec<Pmt>>(v)
            .map(Pmt::VecPmt)
            .map_err(|e| format!("expected JSON array of PMTs: {e}")),
        PmtKind::MapStrPmt => serde_json::from_str::<HashMap<String, Pmt>>(v)
            .map(Pmt::MapStrPmt)
            .map_err(|e| format!("expected JSON object map string->PMT: {e}")),
        _ => Err("unsupported PMT kind for JSON submission".to_string()),
    }
}

#[component]
/// Reactive textual representation of PMT.
pub fn Pmt(
    #[prop(into)] pmt: Signal<Pmt>,
    #[prop(into, optional)] span_class: String,
) -> impl IntoView {
    let class = {
        move || {
            let c = match pmt() {
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
            };
            format!("{c} {span_class}")
        }
    };

    view! { <span class=class>{move || pmt.get().to_string()}</span> }
}

#[component]
pub fn PmtEditor(
    on_submit: Callback<Pmt>,
    #[prop(optional)] disabled: bool,
    #[prop(into, optional)] select_class: String,
    #[prop(into, optional)] input_class: String,
    #[prop(into, optional)] error_class: String,
    #[prop(into, optional)] button_class: String,
    #[prop(into, optional, default = "Submit".to_string())] button_text: String,
) -> impl IntoView {
    let (error, set_error) = signal(None::<String>);

    let select_ref = NodeRef::<Select>::new();
    let input_ref = NodeRef::<Textarea>::new();

    let parse_and_submit = move || {
        set_error(None);
        let t = select_ref
            .get()
            .and_then(|s| s.value().parse::<PmtKind>().ok())
            .unwrap_or(PmtKind::Null);
        let v = input_ref.get().map(|i| i.value()).unwrap_or_default();

        match parse_json_pmt(t, &v) {
            Ok(pmt) => on_submit.run(pmt),
            Err(e) => set_error(Some(e)),
        }
    };

    view! {
        <div class="flex flex-col gap-2">
            <select node_ref=select_ref class=select_class>
                {JSON_PMT_TYPES
                    .into_iter()
                    .map(|k| view! { <option value=k.to_string()>{k.to_string()}</option> })
                    .collect::<Vec<_>>()}
            </select>
            <textarea
                node_ref=input_ref
                class=input_class
                placeholder="value; use JSON for arrays/maps"
            ></textarea>
            <div class=error_class>{move || error.get().unwrap_or_default()}</div>
            <button class=button_class on:click=move |_| parse_and_submit() disabled=disabled>
                {button_text}
            </button>
        </div>
    }
}

const ENTER_KEY: u32 = 13;

#[component]
/// Input a PMT
pub fn PmtInput(
    set_pmt: WriteSignal<Pmt>,
    #[prop(default = false)] button: bool,
    #[prop(into, optional)] input_class: String,
    #[prop(into, optional)] error_class: String,
    #[prop(into, optional)] button_class: String,
    #[prop(into, optional, default = "Submit".to_string())] button_text: String,
) -> impl IntoView {
    let (error, set_error) = signal(false);
    let classes = Memo::new(move |_| {
        if error() {
            format!("{input_class} {error_class}")
        } else {
            input_class.to_string()
        }
    });

    let input_ref = NodeRef::<Input>::new();
    let parse_pmt = move || {
        let input = input_ref.get().unwrap();
        let v = input.value();
        match v.parse::<Pmt>() {
            Ok(p) => {
                set_pmt(p);
            }
            _ => {
                set_error(true);
            }
        }
    };

    let on_input = move |ev: web_sys::KeyboardEvent| {
        ev.stop_propagation();
        set_error(false);
        let key_code = ev.key_code();
        if key_code == ENTER_KEY {
            parse_pmt();
        }
    };

    view! {
        <input class=classes node_ref=input_ref on:keydown=on_input />
        {move || {
            button
                .then({
                    let button_text = button_text.clone();
                    let button_class = button_class.clone();
                    move || {
                        view! {
                            <button class=button_class on:click=move |_| parse_pmt()>
                                {button_text}
                            </button>
                        }
                    }
                })
        }}
    }
}

#[component]
/// PMT Input with list for type selection
pub fn PmtInputList(
    set_pmt: WriteSignal<Pmt>,
    #[prop(default = JSON_PMT_TYPES.to_vec())] types: Vec<PmtKind>,
    #[prop(default = false)] button: bool,
    #[prop(into, optional)] input_class: String,
    #[prop(into, optional)] error_class: String,
    #[prop(into, optional)] button_class: String,
    #[prop(into, optional)] select_class: String,
    #[prop(into, optional, default = "Submit".to_string())] button_text: String,
) -> impl IntoView {
    let (error, set_error) = signal(false);
    let classes = Memo::new(move |_| {
        if error() {
            format!("{input_class} {error_class}")
        } else {
            input_class.to_string()
        }
    });

    let input_ref = NodeRef::<Input>::new();
    let select_ref = NodeRef::<Select>::new();

    let parse_pmt = move || {
        set_error(false);
        let v = input_ref.get().unwrap().value();
        let t = select_ref
            .get()
            .unwrap()
            .value()
            .parse::<PmtKind>()
            .unwrap_or(PmtKind::Null);
        match parse_json_pmt(t, &v) {
            Ok(pmt) => set_pmt(pmt),
            Err(_) => set_error(true),
        }
    };

    let on_input = move |ev: web_sys::KeyboardEvent| {
        ev.stop_propagation();
        set_error(false);
        let key_code = ev.key_code();
        if key_code == ENTER_KEY {
            parse_pmt();
        }
    };

    view! {
        <select node_ref=select_ref class=select_class>
            {types
                .into_iter()
                .map(|k| view! { <option value=k.to_string()>{k.to_string()}</option> })
                .collect::<Vec<_>>()}
        </select>
        <input class=classes node_ref=input_ref on:keydown=on_input />
        {move || {
            button
                .then({
                    let button_text = button_text.clone();
                    let button_class = button_class.clone();
                    move || {
                        view! {
                            <button class=button_class on:click=move |_| parse_pmt()>
                                {button_text}
                            </button>
                        }
                    }
                })
        }}
    }
}
