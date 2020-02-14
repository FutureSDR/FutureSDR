use anyhow::{Error, Result};
use http::request::Request;
use http::response::Response;
use yew::format::Json;
use yew::prelude::*;
use yew::services::fetch::{FetchService, FetchTask};
use yew::services::ConsoleService;

use futuresdr_pmt::Pmt;
use futuresdr_pmt::PmtKind;

pub enum Msg {
    Submit,
    Ignore,
    Error,
    Value(String),
    Result(String),
}

#[derive(Clone, Properties, PartialEq)]
pub struct Props {
    pub url: String,
    pub block: u64,
    pub callback: u64,
    pub pmt_type: PmtKind,
}

pub struct Call {
    link: ComponentLink<Self>,
    props: Props,
    input: String,
    result: String,
    error: bool,
    fetch_task: Option<FetchTask>,
}

impl Call {
    fn endpoint(props: &Props) -> String {
        format!(
            "{}/api/block/{}/call/{}",
            props.url, props.block, props.callback
        )
    }

    fn fetch(props: &Props, link: &ComponentLink<Self>, p: &Pmt) -> Option<FetchTask> {
        let request = Request::post(&Self::endpoint(props))
            .header("Content-Type", "application/json")
            .body(Json(p));
        if request.is_err() {
            ConsoleService::debug("creating request failed");
            return None;
        }
        let request = request.unwrap();

        if let Ok(t) = FetchService::fetch(
            request,
            link.callback(|response: Response<Result<String, Error>>| {
                if response.status().is_success() {
                    Msg::Result(response.into_body().unwrap())
                } else {
                    Msg::Error
                }
            }),
        ) {
            Some(t)
        } else {
            None
        }
    }
}

impl Component for Call {
    type Message = Msg;
    type Properties = Props;

    fn create(props: Self::Properties, link: ComponentLink<Self>) -> Self {
        Self {
            link,
            props,
            input: "".to_string(),
            result: "<none>".to_string(),
            error: false,
            fetch_task: None,
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::Submit => {
                ConsoleService::log(&format!("submitting: {}", self.input));
                if let Some(p) = Pmt::from_string(&self.input, &self.props.pmt_type) {
                    ConsoleService::log(&format!("pmt: {:?}", p));
                    self.fetch_task = Self::fetch(&self.props, &self.link, &p);
                } else {
                    self.result = "Parse Error".to_string();
                    self.error = true;
                    self.fetch_task = None;
                }
            }
            Msg::Value(v) => {
                self.input = v;
            }
            Msg::Error => {
                self.result = "Error".to_string();
                self.fetch_task = None;
                self.error = true;
            }
            Msg::Result(v) => {
                self.result = v;
                self.fetch_task = None;
                self.error = false;
            }
            Msg::Ignore => {}
        };
        true
    }

    fn change(&mut self, props: Self::Properties) -> ShouldRender {
        if props == self.props {
            return false;
        }

        self.props = props;
        true
    }

    fn view(&self) -> Html {
        let mut classes = "".to_string();
        if self.fetch_task.is_some() {
            classes.push_str(" fetching");
        }
        if self.error {
            classes.push_str(" error");
        }
        html! {
            <div>
                <input class="edit"
                    type="text"
                    value=self.input.to_string()
                    oninput=self.link.callback(|e: InputData| Msg::Value(e.value))
                    onkeypress=self.link.callback(move |e: KeyboardEvent| {
                        if e.key() == "Enter" { Msg::Submit } else { Msg::Ignore }
                    })/>
                <span class={classes}>{ &self.result }</span>
            </div>
        }
    }
}
