use serde::ser::SerializeTuple;
use serde::ser::Serializer;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::WorkerGlobalScope;

use crate::anyhow::Result;
use crate::num_complex::Complex32;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Pmt;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

const TRANSFER_SIZE: usize = 262144;

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

#[allow(dead_code)]
#[repr(u8)]
enum TransceiverMode {
    Off = 0,
    Receive = 1,
    Transmit = 2,
    SS = 3,
    CpldUpdate = 4,
    RxSweep = 5,
}

#[derive(thiserror::Error, Debug, Clone)]
pub enum Error {
    #[error("Argument")]
    Argument,
    #[error("Browser error")]
    BrowserError(String),
}

impl From<TransceiverMode> for u8 {
    fn from(m: TransceiverMode) -> Self {
        m as u8
    }
}

impl From<TransceiverMode> for u16 {
    fn from(m: TransceiverMode) -> Self {
        m as u16
    }
}

impl From<JsValue> for Error {
    fn from(e: JsValue) -> Self {
        Self::BrowserError(format!("{:?}", e))
    }
}

/// WASM-native HackRf Source
pub struct HackRf {
    buffer: [i8; TRANSFER_SIZE],
    offset: usize,
    device: Option<web_sys::UsbDevice>,
}

unsafe impl Send for HackRf {}

impl HackRf {
    /// Create HackRf Source
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("HackRf").build(),
            StreamIoBuilder::new()
                .add_output::<Complex32>("out")
                .build(),
            MessageIoBuilder::<Self>::new()
                .add_input("freq", Self::freq_handler)
                .add_input("vga", Self::vga_handler)
                .add_input("lna", Self::lna_handler)
                .add_input("amp", Self::amp_handler)
                .add_input("sample_rate", Self::sample_rate_handler)
                .build(),
            Self {
                buffer: [0; TRANSFER_SIZE],
                offset: TRANSFER_SIZE,
                device: None,
            },
        )
    }

    #[message_handler]
    fn freq_handler(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let res = match &p {
            Pmt::F32(v) => self.set_freq(*v as u64).await,
            Pmt::F64(v) => self.set_freq(*v as u64).await,
            Pmt::U32(v) => self.set_freq(*v as u64).await,
            Pmt::U64(v) => self.set_freq(*v).await,
            _ => return Ok(Pmt::InvalidValue),
        };
        if res.is_ok() {
            Ok(Pmt::Ok)
        } else {
            Ok(Pmt::InvalidValue)
        }
    }

    #[message_handler]
    fn lna_handler(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let res = match &p {
            Pmt::F32(v) => self.set_lna_gain(*v as u16).await,
            Pmt::F64(v) => self.set_lna_gain(*v as u16).await,
            Pmt::U32(v) => self.set_lna_gain(*v as u16).await,
            Pmt::U64(v) => self.set_lna_gain(*v as u16).await,
            _ => return Ok(Pmt::InvalidValue),
        };
        if res.is_ok() {
            Ok(Pmt::Ok)
        } else {
            Ok(Pmt::InvalidValue)
        }
    }

    #[message_handler]
    fn vga_handler(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let res = match &p {
            Pmt::F32(v) => self.set_vga_gain(*v as u16).await,
            Pmt::F64(v) => self.set_vga_gain(*v as u16).await,
            Pmt::U32(v) => self.set_vga_gain(*v as u16).await,
            Pmt::U64(v) => self.set_vga_gain(*v as u16).await,
            _ => return Ok(Pmt::InvalidValue),
        };
        if res.is_ok() {
            Ok(Pmt::Ok)
        } else {
            Ok(Pmt::InvalidValue)
        }
    }

    #[message_handler]
    fn amp_handler(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let res = match &p {
            Pmt::Bool(b) => self.set_amp_enable(*b).await,
            _ => return Ok(Pmt::InvalidValue),
        };
        if res.is_ok() {
            Ok(Pmt::Ok)
        } else {
            Ok(Pmt::InvalidValue)
        }
    }

    #[message_handler]
    fn sample_rate_handler(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let res = match &p {
            Pmt::F32(v) => self.set_sample_rate_auto(*v as f64).await,
            Pmt::F64(v) => self.set_sample_rate_auto(*v).await,
            Pmt::U32(v) => self.set_sample_rate_auto(*v as f64).await,
            Pmt::U64(v) => self.set_sample_rate_auto(*v as f64).await,
            _ => return Ok(Pmt::InvalidValue),
        };
        if res.is_ok() {
            Ok(Pmt::Ok)
        } else {
            Ok(Pmt::InvalidValue)
        }
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
            value,
        );

        let transfer = self
            .device
            .as_ref()
            .unwrap()
            .control_transfer_in(&parameter, N as u16);

        let data = JsFuture::from(transfer)
            .await?
            .dyn_into::<web_sys::UsbInTransferResult>()
            .unwrap()
            .data()
            .unwrap()
            .dyn_into::<js_sys::DataView>()
            .unwrap();

        for (i, b) in buf.iter_mut().enumerate().take(N) {
            *b = data.get_uint8(i);
        }

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
            value,
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

    // Helper for set_freq
    fn freq_params(hz: u64) -> [u8; 8] {
        const MHZ: u64 = 1_000_000;

        let l_freq_mhz: u32 = u32::try_from(hz / MHZ).unwrap_or(u32::MAX).to_le();
        let l_freq_hz: u32 = u32::try_from(hz - u64::from(l_freq_mhz) * MHZ)
            .unwrap_or(u32::MAX)
            .to_le();

        [
            (l_freq_mhz & 0xFF) as u8,
            ((l_freq_mhz >> 8) & 0xFF) as u8,
            ((l_freq_mhz >> 16) & 0xFF) as u8,
            ((l_freq_mhz >> 24) & 0xFF) as u8,
            (l_freq_hz & 0xFF) as u8,
            ((l_freq_hz >> 8) & 0xFF) as u8,
            ((l_freq_hz >> 16) & 0xFF) as u8,
            ((l_freq_hz >> 24) & 0xFF) as u8,
        ]
    }

    async fn set_freq(&mut self, hz: u64) -> Result<(), Error> {
        let mut buf: [u8; 8] = Self::freq_params(hz);
        self.write_control(Request::SetFreq, 0, 0, &mut buf).await
    }

    async fn set_hw_sync_mode(&mut self, value: u8) -> Result<(), Error> {
        self.write_control(Request::SetHwSyncMode, value.into(), 0, &mut [])
            .await
    }

    async fn set_amp_enable(&mut self, en: bool) -> Result<(), Error> {
        self.write_control(Request::AmpEnable, en.into(), 0, &mut [])
            .await
    }

    async fn set_baseband_filter_bandwidth(&mut self, hz: u32) -> Result<(), Error> {
        self.write_control(
            Request::BasebandFilterBandwidthSet,
            (hz & 0xFFFF) as u16,
            (hz >> 16) as u16,
            &mut [],
        )
        .await
    }

    const MAX_N: usize = 32;

    #[allow(unused_assignments)]
    async fn set_sample_rate_auto(&mut self, freq: f64) -> Result<(), Error> {
        let freq_frac = 1.0 + freq - freq.trunc();

        let mut d = freq;
        let u = unsafe { &mut *(&mut d as *mut f64 as *mut u64) };
        let e = (*u >> 52) - 1023;
        let mut m = (1u64 << 52) - 1;

        d = freq_frac;
        *u &= m;
        m &= !((1 << (e + 4)) - 1);
        let mut a = 0;

        let mut i = 1;
        for _ in 1..Self::MAX_N {
            a += *u;
            if ((a & m) == 0) || ((!a & m) == 0) {
                break;
            }
            i += 1;
        }

        if i == Self::MAX_N {
            i = 1;
        }

        let freq_hz = (freq * i as f64 + 0.5).trunc() as u32;
        let divider = i as u32;

        self.set_sample_rate(freq_hz, divider).await
    }

    async fn set_sample_rate(&mut self, hz: u32, div: u32) -> Result<(), Error> {
        let hz: u32 = hz.to_le();
        let div: u32 = div.to_le();
        let mut buf: [u8; 8] = [
            (hz & 0xFF) as u8,
            ((hz >> 8) & 0xFF) as u8,
            ((hz >> 16) & 0xFF) as u8,
            ((hz >> 24) & 0xFF) as u8,
            (div & 0xFF) as u8,
            ((div >> 8) & 0xFF) as u8,
            ((div >> 16) & 0xFF) as u8,
            ((div >> 24) & 0xFF) as u8,
        ];
        self.write_control(Request::SampleRateSet, 0, 0, &mut buf)
            .await?;
        self.set_baseband_filter_bandwidth((0.75 * (hz as f32) / (div as f32)) as u32)
            .await
    }

    async fn set_transceiver_mode(&mut self, mode: TransceiverMode) -> Result<(), Error> {
        self.write_control(Request::SetTransceiverMode, mode.into(), 0, &mut [])
            .await
    }

    async fn set_lna_gain(&mut self, gain: u16) -> Result<(), Error> {
        if gain > 40 {
            Err(Error::Argument)
        } else {
            let buf: [u8; 1] = self
                .read_control(Request::SetLnaGain, 0, gain & !0x07)
                .await?;
            if buf[0] == 0 {
                Err(Error::Argument)
            } else {
                Ok(())
            }
        }
    }

    async fn set_vga_gain(&mut self, gain: u16) -> Result<(), Error> {
        if gain > 62 {
            Err(Error::Argument)
        } else {
            let buf: [u8; 1] = self
                .read_control(Request::SetVgaGain, 0, gain & !0b1)
                .await?;
            if buf[0] == 0 {
                Err(Error::Argument)
            } else {
                Ok(())
            }
        }
    }

    async fn fill_buffer(&mut self) -> Result<(), Error> {
        let transfer = self
            .device
            .as_ref()
            .unwrap()
            .transfer_in(1, TRANSFER_SIZE as u32);

        let data = JsFuture::from(transfer)
            .await?
            .dyn_into::<web_sys::UsbInTransferResult>()
            .unwrap()
            .data()
            .unwrap()
            .dyn_into::<js_sys::DataView>()
            .unwrap();

        debug_assert_eq!(data.byte_length(), TRANSFER_SIZE);
        for i in 0..TRANSFER_SIZE {
            self.buffer[i] = data.get_int8(i);
        }
        self.offset = 0;

        Ok(())
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
        let usb = {
            if let Some(window) = web_sys::window() {
                let navigator: web_sys::Navigator = window.navigator();
                navigator.usb()
            } else {
                let scope : WorkerGlobalScope = js_sys::global().dyn_into().expect("Neither window nor Worker context exists.");
                let navigator = scope.navigator();
                navigator.usb()
            }
        };

        let filter: serde_json::Value = serde_json::from_str(r#"{ "vendorId": 7504 }"#).unwrap();
        let s = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
        let mut tup = s.serialize_tuple(1).unwrap();
        tup.serialize_element(&filter).unwrap();
        let filter = tup.end().unwrap();
        let filter = web_sys::UsbDeviceRequestOptions::new(filter.as_ref());

        let devices: js_sys::Array = JsFuture::from(usb.get_devices())
            .await
            .map_err(Error::from)?
            .into();

        for i in 0..devices.length() {
            let d: web_sys::UsbDevice = devices.get(0).dyn_into().unwrap();
            println!("dev {}   {:?}", i, &d);
        }
        // Open radio if one is already paired and plugged
        // Otherwise ask the user to pair a new radio
        let device: web_sys::UsbDevice = if devices.length() > 0 {
            info!("device already connected");
            devices.get(0).dyn_into().unwrap()
        } else {
            info!("requesting device: {:?}", &filter);
            JsFuture::from(usb.request_device(&filter))
                .await
                .map_err(Error::from)?
                .dyn_into()
                .map_err(Error::from)?
        };

        info!("opening device");
        JsFuture::from(device.open()).await.map_err(Error::from)?;
        info!("selecting configuration");
        JsFuture::from(device.select_configuration(1))
            .await
            .map_err(Error::from)?;

        info!("claiming device");
        JsFuture::from(device.claim_interface(0))
            .await
            .map_err(Error::from)?;

        self.device = Some(device);
        self.set_sample_rate(8_000_000, 2).await.unwrap();
        self.set_hw_sync_mode(0).await.unwrap();
        self.set_freq(2_480_000_000).await.unwrap();
        self.set_vga_gain(4).await.unwrap();
        self.set_lna_gain(24).await.unwrap();
        self.set_amp_enable(true).await.unwrap();
        self.set_transceiver_mode(TransceiverMode::Receive)
            .await
            .unwrap();

        Ok(())
    }

    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let o = sio.output(0).slice::<Complex32>();

        let n = std::cmp::min(o.len(), (TRANSFER_SIZE - self.offset) / 2);

        for (i, out) in o.iter_mut().enumerate().take(n) {
            *out = Complex32::new(
                (self.buffer[self.offset + i * 2] as f32) / 128.0,
                (self.buffer[self.offset + i * 2 + 1] as f32) / 128.0,
            );
        }

        sio.output(0).produce(n);
        self.offset += n * 2;
        if self.offset == TRANSFER_SIZE {
            self.fill_buffer().await.unwrap();
            io.call_again = true;
        }

        Ok(())
    }
}
