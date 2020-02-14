use anyhow::{Error, Result};
use http::request::Request;
use http::response::Response;
use std::convert::TryFrom;
use yew::format::Json;
use yew::prelude::*;
use yew::services::fetch::{FetchService, FetchTask};
use yew::services::ConsoleService;

use futuresdr_pmt::Pmt;
use futuresdr_pmt::PmtKind;

pub enum Msg {
    Error,
    Value(yew::events::ChangeData),
    Result(String),
}

#[derive(Clone, Properties, PartialEq)]
pub struct Props {
    pub url: String,
    pub block: u32,
    pub callback: u32,
    pub pmt_type: PmtKind,
    pub min: i64,
    pub max: i64,
    pub step: i64,
}

pub struct Slider {
    link: ComponentLink<Self>,
    props: Props,
    value: i64,
    result: String,
    error: bool,
    fetch_task: Option<FetchTask>,
}

impl Slider {
    fn endpoint(props: &Props) -> String {
        format!(
            "{}/api/block/{}/call/{}",
            props.url, props.block, props.callback
        )
    }

    fn callback(props: &Props, link: &ComponentLink<Self>, p: &Pmt) -> Option<FetchTask> {
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

    fn value_to_pmt(value: i64, props: &Props) -> Option<Pmt> {
        match props.pmt_type {
            PmtKind::U32 => {
                let v = u32::try_from(value).ok()?;
                Some(Pmt::U32(v))
            }
            PmtKind::U64 => {
                let v = u64::try_from(value).ok()?;
                Some(Pmt::U64(v))
            }
            PmtKind::Double => Some(Pmt::Double(value as f64)),
            _ => None,
        }
    }
}

impl Component for Slider {
    type Message = Msg;
    type Properties = Props;

    fn create(props: Self::Properties, link: ComponentLink<Self>) -> Self {
        let mut error = false;
        let mut result = "<fetching>".to_string();
        let value = props.min;
        let mut fetch_task = None;

        if let Some(p) = Self::value_to_pmt(value, &props) {
            fetch_task = Self::callback(&props, &link, &p);
            if fetch_task.is_none() {
                error = true;
                result = "Error".to_string();
            }
        } else {
            error = true;
            result = "Error".to_string();
        }

        Self {
            link,
            props,
            value,
            result,
            error,
            fetch_task,
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::Value(v) => {
                self.error = false;
                self.result = "<calling>".to_string();

                if let ChangeData::Value(s) = v {
                    if let Ok(v) = s.parse::<i64>() {
                        self.value = v;

                        if let Some(p) = Self::value_to_pmt(self.value, &self.props) {
                            self.fetch_task = Self::callback(&self.props, &self.link, &p);
                            if self.fetch_task.is_some() {
                                return true;
                            }
                        }
                    }
                }

                self.error = true;
                self.result = "Error".to_string();
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
                <input type="range"
                value=self.value.to_string()
                min=self.props.min.to_string()
                max=self.props.max.to_string()
                step=self.props.step.to_string()
                onchange=self.link.callback(Msg::Value)
                />
                <span class={classes}>{ &self.result }</span>
            </div>
        }
    }
}
