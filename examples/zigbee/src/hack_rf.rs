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

#[allow(dead_code)]
#[repr(u8)]
enum Request {
    SetTransceiverMode = 1,
    Max2837Write = 2,
    Max2837Read = 3,
    Si5351CWrite = 4,
    Si5351CRead = 5,
    SampleRateSet = 6,
    BasebandFilterBandwidthSet = 7,
    Rffc5071Write = 8,
    Rffc5071Read = 9,
    SpiflashErase = 10,
    SpiflashWrite = 11,
    SpiflashRead = 12,
    BoardIdRead = 14,
    VersionStringRead = 15,
    SetFreq = 16,
    AmpEnable = 17,
    BoardPartidSerialnoRead = 18,
    SetLnaGain = 19,
    SetVgaGain = 20,
    SetTxvgaGain = 21,
    AntennaEnable = 23,
    SetFreqExplicit = 24,
    UsbWcidVendorReq = 25,
    InitSweep = 26,
    OperacakeGetBoards = 27,
    OperacakeSetPorts = 28,
    SetHwSyncMode = 29,
    Reset = 30,
    OperacakeSetRanges = 31,
    ClkoutEnable = 32,
    SpiflashStatus = 33,
    SpiflashClearStatus = 34,
    OperacakeGpioTest = 35,
    CpldChecksum = 36,
    UiEnable = 37,
}

impl From<Request> for u8 {
    fn from(r: Request) -> Self {
        r as u8
    }
}

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

    async fn read_control<const N: usize>(
        &self,
        request: Request,
        value: u16,
        index: u16,
    ) -> Result<[u8; N], Error> {
        let mut buf: [u8; N] = [0; N];
        let parameter = web_sys::UsbControlTransferParameters::new(
            index,
            web_sys::UsbRecipient::Device,
            request.into(),
            web_sys::UsbRequestType::Vendor,
            value.into(),
        );

        let transfer = self
            .device
            .as_ref()
            .unwrap()
            .control_transfer_out_with_u8_array(&parameter, &mut buf);

        let _ = JsFuture::from(transfer)
            .await?
            .dyn_into::<web_sys::UsbOutTransferResult>()
            .unwrap();

        Ok(buf)
    }

    async fn write_control(
        &mut self,
        request: Request,
        value: u16,
        index: u16,
        buf: &mut [u8],
    ) -> Result<(), Error> {
        let parameter = web_sys::UsbControlTransferParameters::new(
            index,
            web_sys::UsbRecipient::Device,
            request.into(),
            web_sys::UsbRequestType::Vendor,
            value.into(),
        );

        let transfer = self
            .device
            .as_ref()
            .unwrap()
            .control_transfer_out_with_u8_array(&parameter, buf);

        let _ = JsFuture::from(transfer)
            .await?
            .dyn_into::<web_sys::UsbOutTransferResult>()
            .unwrap();

        Ok(())
    }

    async fn set_freq(&mut self, hz: u64) -> Result<(), Error> {
        let buf: [u8; 8] = freq_params(hz);
        self.write_control(Request::SetFreq, 0, 0, &buf)
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
            serde_json::from_str(r#"{ "filters": [{ "vendorId": 0x1D50 }] }"#).unwrap();
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

        JsFuture::from(device.open()).await.map_err(Error::from)?;
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
