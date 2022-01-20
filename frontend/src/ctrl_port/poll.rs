use anyhow::{Error, Result};
use http::request::Request;
use http::response::Response;
use yew::format::Nothing;
use yew::prelude::*;
use yew::services::fetch::{FetchService, FetchTask};
use yew::services::ConsoleService;

pub enum Msg {
    Poll,
    Error,
    Update(String),
}

#[derive(Clone, Properties, Default, PartialEq)]
pub struct Props {
    pub url: String,
    pub block: u64,
    pub callback: u64,
}

pub struct Poll {
    link: ComponentLink<Self>,
    props: Props,
    value: String,
    error: bool,
    fetch_task: Option<FetchTask>,
}

impl Poll {
    fn endpoint(props: &Props) -> String {
        format!(
            "{}/api/block/{}/call/{}/",
            props.url, props.block, props.callback
        )
    }

    fn fetch(props: &Props, link: &ComponentLink<Self>) -> Option<FetchTask> {
        if let Ok(request) = Request::get(&Self::endpoint(props)).body(Nothing) {
            if let Ok(t) = FetchService::fetch(
                request,
                link.callback(|response: Response<Result<String, Error>>| {
                    if response.status().is_success() {
                        Msg::Update(response.into_body().unwrap())
                    } else {
                        Msg::Error
                    }
                }),
            ) {
                Some(t)
            } else {
                ConsoleService::debug("creating fetch task failed");
                None
            }
        } else {
            ConsoleService::debug("creating request failed");
            None
        }
    }
}

impl Component for Poll {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let fetch_task = Self::fetch(&props, &link);
        let error = fetch_task.is_none();
        let value = if error {
            "Error".to_string()
        } else {
            "fetching...".to_string()
        };

        Self {
            link,
            props,
            value,
            error,
            fetch_task,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Poll => {
                self.fetch_task = Self::fetch(&self.props, &self.link);
                self.error = self.fetch_task.is_none();
                if self.error {
                    self.value = "Error".to_string();
                }
            }
            Msg::Error => {
                self.fetch_task = None;
                self.value = "Error".to_string();
                self.error = true;
            }
            Msg::Update(s) => {
                self.error = false;
                self.value = s;
                self.fetch_task = None;
            }
        };
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let mut classes = "".to_string();
        if self.fetch_task.is_some() {
            classes.push_str(" fetching");
        }
        if self.error {
            classes.push_str(" error");
        }
        html! {
            <div>
                <button onclick=self.link.callback(|_| Msg::Poll)>{ "Update" }</button>
                <span class={classes}>{ &self.value }</span>
            </div>
        }
    }
}
