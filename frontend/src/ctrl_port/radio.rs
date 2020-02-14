use anyhow::{Error, Result};
use futuresdr_pmt::Pmt;
use http::request::Request;
use http::response::Response;
use yew::format::Json;
use yew::prelude::*;
use yew::services::fetch::{FetchService, FetchTask};
use yew::services::ConsoleService;

#[derive(Clone, Properties, PartialEq)]
pub struct RadioItemProps {
    pub value: Pmt,
    pub id: String,
}
pub enum RadioItemMsg {
    Clicked,
}
#[derive(Clone)]
pub struct RadioItem {
    props: RadioItemProps,
    link: ComponentLink<Self>,
}

impl Component for RadioItem {
    type Message = RadioItemMsg;
    type Properties = RadioItemProps;

    fn create(props: Self::Properties, link: ComponentLink<Self>) -> Self {
        Self { props, link }
    }

    fn update(&mut self, _msg: Self::Message) -> ShouldRender {
        let parent = self.link.get_parent().unwrap().clone();
        let radio_scope = parent.downcast::<Radio>();
        radio_scope.send_message(Msg::Value(self.props.value.clone()));
        false
    }

    fn change(&mut self, props: Self::Properties) -> ShouldRender {
        if props == self.props {
            return false;
        }
        self.props = props;
        true
    }

    fn view(&self) -> Html {
        let parent = self.link.get_parent().unwrap().clone();
        let radio_scope = parent.downcast::<Radio>();
        let radio = radio_scope.get_component().unwrap();
        let name = radio.props.name.clone();

        html! {
            <>
            <input type="radio" id={self.props.id.to_string()} name={name} onclick=self.link.callback(|_| RadioItemMsg::Clicked) />
            <label for={self.props.id.to_string()}>{format!("{:?}", &self.props.value)}</label>
            </>
        }
    }
}

#[derive(Clone, Properties, PartialEq)]
pub struct Props {
    pub children: ChildrenWithProps<RadioItem>,
    pub url: String,
    pub block: u64,
    pub callback: u64,
    pub name: String,
}

pub struct Radio {
    props: Props,
    link: ComponentLink<Self>,
    value: Option<Pmt>,
    result: String,
    error: bool,
    fetch_task: Option<FetchTask>,
}

pub enum Msg {
    Submit,
    Value(Pmt),
    Result(String),
    Error,
}

impl Radio {
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
}

impl Component for Radio {
    type Message = Msg;
    type Properties = Props;

    fn create(props: Self::Properties, link: ComponentLink<Self>) -> Self {
        Self {
            props,
            link,
            result: "<none>".to_string(),
            value: None,
            error: false,
            fetch_task: None,
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::Submit => {
                ConsoleService::log(&format!("submitting {:?}", self.value));
                if let Some(ref v) = self.value {
                    self.fetch_task = Self::callback(&self.props, &self.link, v);
                    if self.fetch_task.is_some() {
                        self.result = "<fetching>".to_string();
                        self.error = false;
                    } else {
                        self.result = "<Error>".to_string();
                        self.error = true;
                    }
                }
            }
            Msg::Value(p) => {
                ConsoleService::log(&format!("updated value {:?}", &p));
                self.value = Some(p);
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
        let mut classes = Vec::new();
        if self.error {
            classes.push("error");
        }
        if self.fetch_task.is_some() {
            classes.push("fetching");
        }
        html! {
            <>
                { for self.props.children.iter() }
                <button type="submit" onclick=self.link.callback(|_|Msg::Submit)>{"Submit"}</button>
                <div class={classes}>{&self.result}</div>
            </>
        }
    }
}
