use futuresdr::anyhow::Result;
use futuresdr::async_trait;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::WorkIo;
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::JsFuture;

#[derive(thiserror::Error, Debug, Clone)]
pub enum Error {
    #[error("Browser error")]
    BrowserError(String),
}

impl From<JsValue> for Error {
    fn from(e: JsValue) -> Self {
        Self::BrowserError(format!("{:?}", e))
    }
}

pub struct HackRf {
    device: Option<web_sys::UsbDevice>,
}

unsafe impl Send for HackRf {}

impl HackRf {
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("HackRf").build(),
            StreamIoBuilder::new()
                .add_output::<Complex32>("out")
                .build(),
            MessageIoBuilder::<Self>::new().build(),
            Self { device: None },
        )
    }
}

#[async_trait]
impl Kernel for HackRf {
    async fn init(
        &mut self,
        _s: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let window = web_sys::window().expect("No global 'window' exists!");
        let navigator: web_sys::Navigator = window.navigator();
        let usb = navigator.usb();

        let filter: serde_json::Value =
            serde_json::from_str(r#"{ "filters": [{ "vendorId": 6421 }] }"#).unwrap();
        let filter = serde_wasm_bindgen::to_value(&filter).unwrap();

        let devices: js_sys::Array = JsFuture::from(usb.get_devices())
            .await
            .map_err(Error::from)?
            .into();

        // Open radio if one is already paired and plugged
        // Otherwise ask the user to pair a new radio
        let device: web_sys::UsbDevice = if devices.length() > 0 {
            devices.get(0).dyn_into().unwrap()
        } else {
            JsFuture::from(usb.request_device(&filter.into()))
                .await
                .map_err(Error::from)?
                .dyn_into()
                .map_err(Error::from)?
        };

        JsFuture::from(device.open())
            .await
            .map_err(Error::from)?;
        JsFuture::from(device.claim_interface(0))
            .await
            .map_err(Error::from)?;

        self.device = Some(device);

        Ok(())
    }
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let _o = sio.output(0).slice::<Complex32>();
        io.finished = true;

        Ok(())
    }
}
