use futuresdr_types::Pmt;
use futuresdr_types::PmtKind;
use leptos::html::Input;
use leptos::html::Select;
use leptos::*;

#[component]
pub fn Pmt(
    #[prop(into)] pmt: MaybeSignal<Pmt>,
    #[prop(into, optional)] span_class: String,
) -> impl IntoView {
    let class = {
        let pmt = pmt.clone();
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
            format!("{} {}", c, span_class)
        }
    };

    view! {
        <span class=class>{ move || pmt.get().to_string() }</span>
    }
}

const ENTER_KEY: u32 = 13;

#[component]
pub fn PmtInput(
    set_pmt: WriteSignal<Pmt>,
    #[prop(default = false)] button: bool,
    #[prop(into, optional)] input_class: String,
    #[prop(into, optional)] error_class: String,
    #[prop(into, optional)] button_class: String,
    #[prop(into, optional, default = "Submit".to_string())] button_text: String,
) -> impl IntoView {
    let (error, set_error) = create_signal(false);
    let classes = create_memo(move |_| {
        if error() {
            format!("{} {}", input_class, error_class)
        } else {
            input_class.to_string()
        }
    });

    let input_ref = create_node_ref::<Input>();
    let parse_pmt = move || {
        let input = input_ref().unwrap();
        let v = input.value();
        if let Ok(p) = v.parse::<Pmt>() {
            set_pmt(p);
        } else {
            set_error(true);
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
        <input class=classes node_ref=input_ref on:keydown=on_input></input>
        { move || button.then({
            let button_text = button_text.clone();
            let button_class = button_class.clone();
            move || view!{<button class=button_class on:click=move |_| parse_pmt()>{button_text}</button>}})
        }
    }
}

#[component]
pub fn PmtInputList(
    set_pmt: WriteSignal<Pmt>,
    #[prop(default = vec![
            PmtKind::Ok,
            PmtKind::InvalidValue,
            PmtKind::Null,
            PmtKind::String,
            PmtKind::Bool,
            PmtKind::Usize,
            PmtKind::U32,
            PmtKind::U64,
            PmtKind::F32,
            PmtKind::F64,
            PmtKind::VecF32,
            PmtKind::VecU64,
            PmtKind::Blob,
            PmtKind::VecPmt,
            PmtKind::Finished,
            PmtKind::MapStrPmt,
    ])]
    types: Vec<PmtKind>,
    #[prop(default = false)] button: bool,
    #[prop(into, optional)] input_class: String,
    #[prop(into, optional)] error_class: String,
    #[prop(into, optional)] button_class: String,
    #[prop(into, optional)] select_class: String,
    #[prop(into, optional, default = "Submit".to_string())] button_text: String,
) -> impl IntoView {
    let (error, set_error) = create_signal(false);
    let classes = create_memo(move |_| {
        if error() {
            format!("{} {}", input_class, error_class)
        } else {
            input_class.to_string()
        }
    });

    let input_ref = create_node_ref::<Input>();
    let select_ref = create_node_ref::<Select>();

    let parse_pmt = move || {
        let v = input_ref().unwrap().value();
        let t = select_ref().unwrap().value();
        let t = t.parse::<PmtKind>().unwrap();
        let pmt = match t {
            PmtKind::Ok => Some(Pmt::Ok),
            PmtKind::InvalidValue => Some(Pmt::InvalidValue),
            PmtKind::Null => Some(Pmt::Null),
            PmtKind::String => {
                let pmt = serde_json::from_str::<Pmt>(&format!("{{\"String\": {}}}", v))
                    .or_else(|_| serde_json::from_str::<Pmt>(&format!("{{\"String\": \"{}\"}}", v)))
                    .unwrap_or(Pmt::String(v));
                Some(pmt)
            }
            PmtKind::Bool => {
                if v == "true" {
                    Some(Pmt::Bool(true))
                } else if v == "false" {
                    Some(Pmt::Bool(false))
                } else {
                    None
                }
            }
            PmtKind::Usize => v.parse::<usize>().map(Pmt::Usize).ok(),
            PmtKind::U32 => v.parse::<u32>().map(Pmt::U32).ok(),
            PmtKind::U64 => v.parse::<u64>().map(Pmt::U64).ok(),
            PmtKind::F32 => v.parse::<f32>().map(Pmt::F32).ok(),
            PmtKind::F64 => v.parse::<f64>().map(Pmt::F64).ok(),
            PmtKind::VecF32 => serde_json::from_str::<Pmt>(&format!("{{\"VecF32\": {}}}", v))
                .or_else(|_| serde_json::from_str::<Pmt>(&format!("{{\"VecF32\": [{}]}}", v)))
                .ok(),
            PmtKind::VecU64 => serde_json::from_str::<Pmt>(&format!("{{\"VecU64\": {}}}", v))
                .or_else(|_| serde_json::from_str::<Pmt>(&format!("{{\"VecU64\": [{}]}}", v)))
                .ok(),
            PmtKind::Blob => serde_json::from_str::<Pmt>(&format!("{{\"Blob\": {}}}", v))
                .or_else(|_| serde_json::from_str::<Pmt>(&format!("{{\"Blob\": [{}]}}", v)))
                .ok(),
            PmtKind::VecPmt => serde_json::from_str::<Pmt>(&format!("{{\"VecPmt\": {}}}", v))
                .or_else(|_| serde_json::from_str::<Pmt>(&format!("{{\"VecPmt\": [{}]}}", v)))
                .ok(),
            PmtKind::Finished => Some(Pmt::Finished),
            PmtKind::MapStrPmt => serde_json::from_str::<Pmt>(&format!("{{\"MapStrPmt\": {}}}", v))
                .or_else(|_| serde_json::from_str::<Pmt>(&format!("{{\"MapStrPmt\": {{{}}}}}", v)))
                .ok(),
            _ => None,
        };
        if let Some(pmt) = pmt {
            set_pmt(pmt);
        } else {
            set_error(true);
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
        <select node_ref=select_ref class={select_class}> {
            types.into_iter()
            .map(|k| view! {
                <option value={k.to_string()}>{k.to_string()}</option>
            })
            .collect::<Vec<_>>()
        }
        </select>
        <input class=classes node_ref=input_ref on:keydown=on_input></input>
        { move || button.then({
            let button_text = button_text.clone();
            let button_class = button_class.clone();
            move || view!{<button class=button_class on:click=move |_| parse_pmt()>{button_text}</button>}})
        }
    }
}
