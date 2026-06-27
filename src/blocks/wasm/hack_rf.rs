use futures::FutureExt;
use std::collections::VecDeque;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::WorkerGlobalScope;

use crate::runtime::dev::prelude::*;

const TRANSFER_SIZE: usize = 262144;
const N_PENDING_TRANSFERS: usize = 8;
const USB_INIT_TIMEOUT_MS: u32 = 10_000;
const HACKRF_VENDOR_ID: u16 = 7504;
const DEFAULT_SAMPLE_RATE_HZ: f64 = 4_000_000.0;

async fn wait_usb<T>(future: JsFuture<T>, operation: &str) -> Result<T, Error> {
    let future = future.fuse();
    let timeout = gloo_timers::future::TimeoutFuture::new(USB_INIT_TIMEOUT_MS).fuse();
    futures::pin_mut!(future);
    futures::pin_mut!(timeout);

    match futures::future::select(future, timeout).await {
        futures::future::Either::Left((result, _)) => result.map_err(Error::from),
        futures::future::Either::Right((_, _)) => Err(Error::BrowserError(format!(
            "{operation} timed out after {USB_INIT_TIMEOUT_MS} ms"
        ))),
    }
}

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
        Self::BrowserError(format!("{e:?}"))
    }
}

/// WASM-native HackRF source.
///
/// # Stream Inputs
///
/// No stream inputs.
///
/// # Stream Outputs
///
/// `output`: Complex I/Q samples from the device.
///
/// # Message Inputs
///
/// `freq`, `vga`, `lna`, `amp`, `sample_rate`, `bandwidth`: Device settings.
///
/// # Usage
/// ```ignore
/// use futuresdr::blocks::wasm::HackRf;
///
/// let source = HackRf::new();
/// ```
#[derive(Block)]
#[message_inputs(
    freq,
    vga,
    lna,
    amp,
    set_sample_rate_msg = "sample_rate",
    set_bandwidth_msg = "bandwidth"
)]
pub struct HackRf {
    #[output]
    output: slab::Writer<Complex32>,
    buffer: Box<[i8]>,
    offset: usize,
    device: Option<web_sys::UsbDevice>,
    pending_transfers: VecDeque<js_sys::Promise<web_sys::UsbInTransferResult>>,
    frequency: u64,
    sample_rate: f64,
    bandwidth: f64,
    vga_gain: u16,
    lna_gain: u16,
    amp: bool,
    transfers_total: u64,
    transfers_since_log: u64,
    samples_since_log: usize,
    samples_produced_total: u64,
    production_logs_remaining: u8,
    low_rate_intervals: u32,
    last_log_ms: f64,
    last_backpressure_log_ms: f64,
    last_overrun_log_ms: f64,
}

impl Default for HackRf {
    fn default() -> Self {
        Self::new()
    }
}

impl HackRf {
    /// Create HackRf Source
    pub fn new() -> Self {
        Self {
            output: slab::Writer::default(),
            buffer: vec![0; TRANSFER_SIZE].into_boxed_slice(),
            offset: TRANSFER_SIZE,
            device: None,
            pending_transfers: VecDeque::new(),
            frequency: 2_480_000_000,
            sample_rate: DEFAULT_SAMPLE_RATE_HZ,
            bandwidth: DEFAULT_SAMPLE_RATE_HZ,
            vga_gain: 2,
            lna_gain: 32,
            amp: true,
            transfers_total: 0,
            transfers_since_log: 0,
            samples_since_log: 0,
            samples_produced_total: 0,
            production_logs_remaining: 5,
            low_rate_intervals: 0,
            last_log_ms: js_sys::Date::now(),
            last_backpressure_log_ms: js_sys::Date::now(),
            last_overrun_log_ms: js_sys::Date::now(),
        }
    }

    /// Request WebUSB permission for a HackRF device from the browser thread.
    ///
    /// Worker-side flowgraphs cannot reliably show the browser device chooser,
    /// so WASM applications should call this from a user gesture before starting
    /// a flowgraph that contains a [`HackRf`] block.
    pub async fn request_permission() -> Result<(), Error> {
        let window = web_sys::window().ok_or_else(|| {
            Error::BrowserError("no browser window available for WebUSB permission request".into())
        })?;
        let usb = window.navigator().usb();

        let devices: js_sys::Array<web_sys::UsbDevice> =
            wait_usb(JsFuture::from(usb.get_devices()), "USB get devices").await?;
        for i in 0..devices.length() {
            let device = devices.get_unchecked(i);
            if device.vendor_id() == HACKRF_VENDOR_ID {
                return Ok(());
            }
        }

        let filter = web_sys::UsbDeviceFilter::new();
        filter.set_vendor_id(HACKRF_VENDOR_ID);
        let filters = [filter];
        let filter = web_sys::UsbDeviceRequestOptions::new(&filters);

        let _: web_sys::UsbDevice = wait_usb(
            JsFuture::from(usb.request_device(&filter)),
            "USB request device",
        )
        .await?;

        Ok(())
    }

    /// Set the initial center frequency in Hz.
    pub fn frequency(mut self, frequency: u64) -> Self {
        self.frequency = frequency;
        self
    }

    /// Set the sample rate in Hz.
    ///
    /// This also sets the baseband filter bandwidth to the same value. To use
    /// a different bandwidth, call [`Self::bandwidth`] after this method.
    pub fn sample_rate(mut self, sample_rate: f64) -> Self {
        self.sample_rate = sample_rate;
        self.bandwidth = sample_rate;
        self
    }

    /// Set the baseband filter bandwidth in Hz.
    pub fn bandwidth(mut self, bandwidth: f64) -> Self {
        self.bandwidth = bandwidth;
        self
    }

    /// Set the initial VGA gain in dB.
    pub fn vga_gain(mut self, gain: u16) -> Self {
        self.vga_gain = gain;
        self
    }

    /// Set the initial LNA gain in dB.
    pub fn lna_gain(mut self, gain: u16) -> Self {
        self.lna_gain = gain;
        self
    }

    /// Enable or disable the RF amplifier initially.
    pub fn amp_enable(mut self, enabled: bool) -> Self {
        self.amp = enabled;
        self
    }

    async fn freq(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let (hz, res) = match &p {
            Pmt::F32(v) => (*v as u64, self.set_freq(*v as u64).await),
            Pmt::F64(v) => (*v as u64, self.set_freq(*v as u64).await),
            Pmt::U32(v) => (*v as u64, self.set_freq(*v as u64).await),
            Pmt::U64(v) => (*v, self.set_freq(*v).await),
            _ => return Ok(Pmt::InvalidValue),
        };
        if res.is_ok() {
            self.frequency = hz;
            self.flush_rx_queue("frequency change");
            info!("HackRF WebUSB frequency set to {hz} Hz");
            Ok(Pmt::Ok)
        } else {
            warn!("HackRF WebUSB failed to set frequency to {hz} Hz: {res:?}");
            Ok(Pmt::InvalidValue)
        }
    }

    async fn lna(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let (gain, res) = match &p {
            Pmt::F32(v) => (*v as u16, self.set_lna_gain(*v as u16).await),
            Pmt::F64(v) => (*v as u16, self.set_lna_gain(*v as u16).await),
            Pmt::U32(v) => (*v as u16, self.set_lna_gain(*v as u16).await),
            Pmt::U64(v) => (*v as u16, self.set_lna_gain(*v as u16).await),
            _ => return Ok(Pmt::InvalidValue),
        };
        if res.is_ok() {
            self.lna_gain = gain;
            self.flush_rx_queue("LNA gain change");
            info!("HackRF WebUSB LNA gain set to {gain} dB");
            Ok(Pmt::Ok)
        } else {
            warn!("HackRF WebUSB failed to set LNA gain to {gain} dB: {res:?}");
            Ok(Pmt::InvalidValue)
        }
    }

    async fn vga(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let (gain, res) = match &p {
            Pmt::F32(v) => (*v as u16, self.set_vga_gain(*v as u16).await),
            Pmt::F64(v) => (*v as u16, self.set_vga_gain(*v as u16).await),
            Pmt::U32(v) => (*v as u16, self.set_vga_gain(*v as u16).await),
            Pmt::U64(v) => (*v as u16, self.set_vga_gain(*v as u16).await),
            _ => return Ok(Pmt::InvalidValue),
        };
        if res.is_ok() {
            self.vga_gain = gain;
            self.flush_rx_queue("VGA gain change");
            info!("HackRF WebUSB VGA gain set to {gain} dB");
            Ok(Pmt::Ok)
        } else {
            warn!("HackRF WebUSB failed to set VGA gain to {gain} dB: {res:?}");
            Ok(Pmt::InvalidValue)
        }
    }

    async fn amp(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let (enabled, res) = match &p {
            Pmt::Bool(b) => (*b, self.set_amp_enable(*b).await),
            _ => return Ok(Pmt::InvalidValue),
        };
        if res.is_ok() {
            self.amp = enabled;
            self.flush_rx_queue("amp change");
            info!("HackRF WebUSB amp set to {enabled}");
            Ok(Pmt::Ok)
        } else {
            warn!("HackRF WebUSB failed to set amp to {enabled}: {res:?}");
            Ok(Pmt::InvalidValue)
        }
    }

    async fn set_sample_rate_msg(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let rate = match &p {
            Pmt::F32(v) => *v as f64,
            Pmt::F64(v) => *v,
            Pmt::U32(v) => *v as f64,
            Pmt::U64(v) => *v as f64,
            _ => return Ok(Pmt::InvalidValue),
        };

        let res = self.set_sample_rate_auto(rate).await;
        if res.is_ok() {
            let bandwidth_res = self.set_baseband_filter_bandwidth(rate as u32).await;
            if bandwidth_res.is_ok() {
                self.sample_rate = rate;
                self.bandwidth = rate;
                self.low_rate_intervals = 0;
                self.flush_rx_queue("sample-rate change");
                info!("HackRF WebUSB sample rate and bandwidth set to {rate} Hz");
                Ok(Pmt::Ok)
            } else {
                warn!(
                    "HackRF WebUSB failed to set default bandwidth to {rate} Hz after sample-rate change: {bandwidth_res:?}"
                );
                Ok(Pmt::InvalidValue)
            }
        } else {
            warn!("HackRF WebUSB failed to set sample rate to {rate} Hz: {res:?}");
            Ok(Pmt::InvalidValue)
        }
    }

    async fn set_bandwidth_msg(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let bandwidth = match &p {
            Pmt::F32(v) => *v as f64,
            Pmt::F64(v) => *v,
            Pmt::U32(v) => *v as f64,
            Pmt::U64(v) => *v as f64,
            _ => return Ok(Pmt::InvalidValue),
        };

        let res = self.set_baseband_filter_bandwidth(bandwidth as u32).await;
        if res.is_ok() {
            self.bandwidth = bandwidth;
            self.flush_rx_queue("bandwidth change");
            info!("HackRF WebUSB bandwidth set to {bandwidth} Hz");
            Ok(Pmt::Ok)
        } else {
            warn!("HackRF WebUSB failed to set bandwidth to {bandwidth} Hz: {res:?}");
            Ok(Pmt::InvalidValue)
        }
    }

    fn flush_rx_queue(&mut self, reason: &str) {
        let queued = self.pending_transfers.len();
        self.pending_transfers.clear();
        self.offset = TRANSFER_SIZE;
        self.buffer.fill(0);
        debug!("HackRF WebUSB flushed RX queue after {reason} ({queued} queued transfers dropped)");
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

        let data = wait_usb(JsFuture::from(transfer), "USB control transfer in")
            .await?
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
        buf: &[u8],
    ) -> Result<(), Error> {
        let parameter = web_sys::UsbControlTransferParameters::new(
            index,
            web_sys::UsbRecipient::Device,
            request.into(),
            web_sys::UsbRequestType::Vendor,
            value,
        );

        // In threaded WASM builds, Rust slices live in a SharedArrayBuffer-backed
        // WebAssembly memory. WebUSB rejects shared ArrayBufferViews for OUT
        // transfers, so copy control payloads into a regular JS Uint8Array.
        let data = js_sys::Uint8Array::new_with_length(buf.len() as u32);
        data.copy_from(buf);

        let transfer = self
            .device
            .as_ref()
            .unwrap()
            .control_transfer_out_with_u8_array(&parameter, &data)
            .unwrap();

        let _ = wait_usb(JsFuture::from(transfer), "USB control transfer out").await?;

        Ok(())
    }

    // Helper for set_freq
    fn freq_params(hz: u64) -> [u8; 8] {
        const MHZ: u64 = 1_000_000;

        let freq_mhz = u32::try_from(hz / MHZ).unwrap_or(u32::MAX);
        let freq_hz =
            u32::try_from(hz.saturating_sub(u64::from(freq_mhz) * MHZ)).unwrap_or(u32::MAX);
        let mut params = [0; 8];
        params[..4].copy_from_slice(&freq_mhz.to_le_bytes());
        params[4..].copy_from_slice(&freq_hz.to_le_bytes());
        params
    }

    async fn set_freq(&mut self, hz: u64) -> Result<(), Error> {
        let buf: [u8; 8] = Self::freq_params(hz);
        self.write_control(Request::SetFreq, 0, 0, &buf).await
    }

    async fn set_hw_sync_mode(&mut self, value: u8) -> Result<(), Error> {
        self.write_control(Request::SetHwSyncMode, value.into(), 0, &[])
            .await
    }

    async fn set_amp_enable(&mut self, en: bool) -> Result<(), Error> {
        self.write_control(Request::AmpEnable, en.into(), 0, &[])
            .await
    }

    async fn set_baseband_filter_bandwidth(&mut self, hz: u32) -> Result<(), Error> {
        self.write_control(
            Request::BasebandFilterBandwidthSet,
            (hz & 0xFFFF) as u16,
            (hz >> 16) as u16,
            &[],
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
        let mut buf = [0; 8];
        buf[..4].copy_from_slice(&hz.to_le_bytes());
        buf[4..].copy_from_slice(&div.to_le_bytes());
        self.write_control(Request::SampleRateSet, 0, 0, &buf).await
    }

    async fn set_transceiver_mode(&mut self, mode: TransceiverMode) -> Result<(), Error> {
        self.write_control(Request::SetTransceiverMode, mode.into(), 0, &[])
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

    fn start_transfer(&mut self) {
        let transfer = self
            .device
            .as_ref()
            .unwrap()
            .transfer_in(1, TRANSFER_SIZE as u32);
        self.pending_transfers.push_back(transfer);
    }

    fn fill_transfer_queue(&mut self) {
        while self.pending_transfers.len() < N_PENDING_TRANSFERS {
            self.start_transfer();
        }
    }

    async fn fill_buffer(&mut self) -> Result<(), Error> {
        self.fill_transfer_queue();
        if self.transfers_total == 0 {
            info!(
                "HackRF WebUSB waiting for first bulk transfer: endpoint 1, {} bytes, {} queued transfers",
                TRANSFER_SIZE,
                self.pending_transfers.len()
            );
        }
        let transfer = self.pending_transfers.pop_front().unwrap();

        let transfer = wait_usb(JsFuture::from(transfer), "USB bulk transfer in")
            .await?
            .dyn_into::<web_sys::UsbInTransferResult>()
            .unwrap();
        let status = transfer.status();
        if status != web_sys::UsbTransferStatus::Ok {
            warn!("HackRF WebUSB bulk transfer status {status:?}; samples may have been lost");
        }
        let data = transfer
            .data()
            .ok_or_else(|| {
                Error::BrowserError(format!(
                    "USB bulk transfer returned {status:?} without data"
                ))
            })?
            .dyn_into::<js_sys::DataView>()
            .unwrap();
        self.fill_transfer_queue();

        let byte_length = data.byte_length();
        if self.transfers_total == 0 {
            info!("HackRF WebUSB received first bulk transfer: {byte_length} bytes");
        }

        if byte_length != TRANSFER_SIZE {
            return Err(Error::BrowserError(format!(
                "short HackRF transfer: received {byte_length} bytes, expected {TRANSFER_SIZE}"
            )));
        }

        let samples = js_sys::Int8Array::new_with_byte_offset_and_length(
            &data.buffer(),
            data.byte_offset() as u32,
            byte_length as u32,
        );
        samples.copy_to(&mut self.buffer);
        self.offset = 0;
        self.transfers_total += 1;
        self.transfers_since_log += 1;
        self.samples_since_log += TRANSFER_SIZE / 2;

        let now = js_sys::Date::now();
        let elapsed_ms = now - self.last_log_ms;
        if elapsed_ms >= 1000.0 {
            let elapsed_s = elapsed_ms / 1000.0;
            let msps = self.samples_since_log as f64 / elapsed_s / 1.0e6;
            info!(
                "HackRF WebUSB RX: {:.2} MS/s, {:.1} transfers/s, total transfers {}",
                msps,
                self.transfers_since_log as f64 / elapsed_s,
                self.transfers_total
            );
            let expected_msps = self.sample_rate / 1.0e6;
            if msps < 0.90 * expected_msps {
                self.low_rate_intervals += 1;
                if self.low_rate_intervals >= 3 && now - self.last_overrun_log_ms >= 5_000.0 {
                    warn!(
                        "HackRF WebUSB sustained RX throughput below requested rate: {:.2}/{:.2} MS/s; samples may be dropped",
                        msps, expected_msps
                    );
                    self.last_overrun_log_ms = now;
                }
            } else {
                self.low_rate_intervals = 0;
            }
            self.samples_since_log = 0;
            self.transfers_since_log = 0;
            self.last_log_ms = now;
        }

        Ok(())
    }
}

#[doc(hidden)]
impl Kernel for HackRf {
    async fn init(&mut self, _mo: &mut MessageOutputs, _b: &BlockMeta) -> Result<()> {
        let usb = {
            if let Some(window) = web_sys::window() {
                let navigator: web_sys::Navigator = window.navigator();
                navigator.usb()
            } else {
                let scope: WorkerGlobalScope = js_sys::global()
                    .dyn_into()
                    .expect("Neither window nor Worker context exists.");
                let navigator = scope.navigator();
                navigator.usb()
            }
        };

        let filter = web_sys::UsbDeviceFilter::new();
        filter.set_vendor_id(HACKRF_VENDOR_ID);
        let filters = [filter];
        let filter = web_sys::UsbDeviceRequestOptions::new(&filters);

        let devices: js_sys::Array<web_sys::UsbDevice> =
            wait_usb(JsFuture::from(usb.get_devices()), "USB get devices").await?;
        let paired_hackrf = (0..devices.length())
            .map(|i| devices.get_unchecked(i))
            .find(|device| device.vendor_id() == HACKRF_VENDOR_ID);
        // Open radio if one is already paired and plugged. Only the browser
        // thread can reliably show a device chooser, so worker-side blocks rely
        // on the application requesting permission before the flowgraph starts.
        let device: web_sys::UsbDevice = if let Some(device) = paired_hackrf {
            device
        } else if web_sys::window().is_some() {
            wait_usb(
                JsFuture::from(usb.request_device(&filter)),
                "USB request device",
            )
            .await?
        } else {
            return Err(Error::BrowserError(
                "no paired HackRF found in worker; request USB permission on the browser thread first"
                    .to_string(),
            )
            .into());
        };

        wait_usb(JsFuture::from(device.open()), "USB open device").await?;
        wait_usb(
            JsFuture::from(device.select_configuration(1)),
            "USB select configuration",
        )
        .await?;
        wait_usb(
            JsFuture::from(device.claim_interface(0)),
            "USB claim interface",
        )
        .await?;

        self.device = Some(device);
        info!(
            "HackRF WebUSB init: frequency {} Hz, sample rate {} Hz, bandwidth {} Hz, LNA {} dB, VGA {} dB, amp {}",
            self.frequency,
            self.sample_rate,
            self.bandwidth,
            self.lna_gain,
            self.vga_gain,
            self.amp
        );

        self.set_sample_rate_auto(self.sample_rate).await?;
        self.set_baseband_filter_bandwidth(self.bandwidth as u32)
            .await?;
        info!(
            "HackRF WebUSB baseband filter bandwidth set to {} Hz",
            self.bandwidth
        );
        self.set_hw_sync_mode(0).await?;
        self.set_freq(self.frequency).await?;
        self.set_vga_gain(self.vga_gain).await?;
        self.set_lna_gain(self.lna_gain).await?;
        self.set_amp_enable(self.amp).await?;
        self.set_transceiver_mode(TransceiverMode::Receive).await?;
        self.last_log_ms = js_sys::Date::now();
        info!("HackRF WebUSB receive mode enabled");

        Ok(())
    }

    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
    ) -> Result<()> {
        let o = self.output.slice();

        let n = std::cmp::min(o.len(), (TRANSFER_SIZE - self.offset) / 2);

        for (i, out) in o.iter_mut().enumerate().take(n) {
            *out = Complex32::new(
                (self.buffer[self.offset + i * 2] as f32) / 128.0,
                (self.buffer[self.offset + i * 2 + 1] as f32) / 128.0,
            );
        }

        if n > 0 {
            self.output.produce(n);
            self.offset += n * 2;
            self.samples_produced_total += n as u64;

            if self.production_logs_remaining > 0 {
                info!(
                    "HackRF WebUSB produced {n} samples (buffer offset {}/{}, total samples {})",
                    self.offset, TRANSFER_SIZE, self.samples_produced_total
                );
                self.production_logs_remaining -= 1;
            }

            if self.offset < TRANSFER_SIZE {
                io.call_again = true;
            }
        } else if self.offset < TRANSFER_SIZE {
            let now = js_sys::Date::now();
            if now - self.last_backpressure_log_ms >= 1000.0 {
                debug!(
                    "HackRF WebUSB output buffer full; applying backpressure (buffer offset {}/{}, total samples {})",
                    self.offset, TRANSFER_SIZE, self.samples_produced_total
                );
                self.last_backpressure_log_ms = now;
            }
        }

        if self.offset == TRANSFER_SIZE {
            self.fill_buffer().await?;
            io.call_again = true;
        }

        Ok(())
    }
}
