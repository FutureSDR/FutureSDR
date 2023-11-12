use thiserror::Error;

// Re-Exports
pub use gloo_net;
pub use gloo_timers;
pub use leptos;

mod array_view;
pub use array_view::ArrayView;

mod constellation_sink;
pub use constellation_sink::ConstellationSink;

mod constellation_sink_density;
pub use constellation_sink_density::ConstellationSinkDensity;

mod handle;
pub use handle::call_periodically;
pub use handle::get_flowgraph_handle;
pub use handle::poll_periodically;
pub use handle::FlowgraphHandle;
pub use handle::RuntimeHandle;

mod flowgraph_canvas;
pub use flowgraph_canvas::FlowgraphCanvas;

mod flowgraph_mermaid;
pub use flowgraph_mermaid::FlowgraphMermaid;

mod list_selector;
pub use list_selector::ListSelector;

mod pmt;
pub use pmt::Pmt;
pub use pmt::PmtInput;
pub use pmt::PmtInputList;

mod radio_selector;
pub use radio_selector::RadioSelector;

mod slider;
pub use slider::Slider;

mod time_sink;
pub use time_sink::TimeSink;
pub use time_sink::TimeSinkMode;

mod waterfall;
pub use waterfall::Waterfall;
pub use waterfall::WaterfallMode;

#[derive(Error, Debug, Clone)]
pub enum Error {
    #[error("Gloo Net Error {0}")]
    Gloo(String),
    #[error("Serde Error {0}")]
    Serde(String),
    #[error("Invalid flowgraph id {0}")]
    FlowgraphId(usize),
    #[error("FutureSDR Error {0}")]
    FutureSdr(#[from] futuresdr::runtime::Error),
}

impl From<gloo_net::Error> for Error {
    fn from(error: gloo_net::Error) -> Self {
        Error::Gloo(format!("{error:?}"))
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Error::Serde(format!("{error:?}"))
    }
}
