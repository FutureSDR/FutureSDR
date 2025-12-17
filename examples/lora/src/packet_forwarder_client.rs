use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;
use std::time::SystemTime;

use chrono::prelude::DateTime;
use chrono::prelude::Utc;
use semtech_udp::Bandwidth;
use semtech_udp::CodingRate;
use semtech_udp::DataRate;
use semtech_udp::MacAddress;
use semtech_udp::SpreadingFactor;
use semtech_udp::client_runtime::Event;
use semtech_udp::client_runtime::UdpRuntime;
use semtech_udp::push_data::CRC;
use semtech_udp::push_data::Packet;
use semtech_udp::push_data::RSig;
use semtech_udp::push_data::RxPk;
use semtech_udp::push_data::RxPkV2;
use tokio::runtime::Runtime;
use triggered::Trigger;

use futuresdr::channel::mpsc::Sender;
use futuresdr::prelude::*;

/// Forward messages.
#[derive(Block)]
#[message_inputs(r#in)]
#[null_kernel]
pub struct PacketForwarderClient {
    mac_addr: MacAddress,
    shutdown_trigger: Trigger,
    uplink_sender: Sender<Packet>,
    #[allow(dead_code)]
    udp_client_runtime: Runtime,
}

impl PacketForwarderClient {
    pub fn new(mac_addr: &str, server_addr: &str) -> Self {
        let mac_address = MacAddress::from_str(mac_addr).unwrap();
        let (to_forwarder_sender, mut to_forwarder_receiver) = mpsc::channel::<Packet>(1);
        let host = SocketAddr::from_str(server_addr).unwrap();
        let server_addr = server_addr.to_owned();

        let (shutdown_trigger, shutdown_signal) = triggered::trigger();
        let shutdown_trigger_tmp = shutdown_trigger.clone();

        let rt_tokio = Runtime::new().unwrap();
        let handle = rt_tokio.handle();

        rt_tokio.block_on(async move {
            let (uplink_sender, mut downlink_request_receiver, udp_runtime) =
                UdpRuntime::new(mac_address, host).await.unwrap();
            println!("Connecting to server {server_addr}");
            let udp_runtime_task = handle.spawn(udp_runtime.run(shutdown_signal));

            // send received frames
            handle.spawn(async move {
                while let Some(received_frame) = to_forwarder_receiver.next().await {
                    uplink_sender.send(received_frame).await.unwrap();
                }
            });

            // report problems with the server connection and handle downlink request acks
            handle.spawn(async move {
                while let Some(event) = downlink_request_receiver.recv().await {
                    match event {
                        Event::LostConnection => {
                            println!("Lost connection to GWMP client");
                        }
                        Event::Reconnected => {
                            println!("Reconnected to GWMP client");
                        }
                        Event::DownlinkRequest(downlink_request) => {
                            downlink_request.ack().await.unwrap()
                        }
                        Event::UnableToParseUdpFrame(parse_error, _buffer) => {
                            println!("Error parsing UDP frame {parse_error}")
                        }
                    }
                }
                shutdown_trigger.trigger();
                if let Err(e) = udp_runtime_task.await {
                    println!("UdpRunTime return error {e}");
                }
            });
        });

        Self {
            mac_addr: mac_address,
            shutdown_trigger: shutdown_trigger_tmp,
            uplink_sender: to_forwarder_sender,
            udp_client_runtime: rt_tokio,
        }
    }

    async fn r#in(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        match p {
            Pmt::Finished => {
                self.shutdown_trigger.trigger();
                io.finished = true;
            }
            Pmt::MapStrPmt(m) => {
                if let Pmt::Blob(payload) = m.get("payload").unwrap() {
                    let codr: CodingRate = match m.get("code_rate").unwrap() {
                        Pmt::Usize(1) => CodingRate::_4_5,
                        Pmt::Usize(2) => CodingRate::_4_6,
                        Pmt::Usize(3) => CodingRate::_4_7,
                        Pmt::Usize(4) => CodingRate::_4_8,
                        _ => {
                            warn!(
                                "invalid Coding Rate in received msg: {:?}, {:?}",
                                m.get("code_rate"),
                                m
                            );
                            return Ok(Pmt::Ok);
                        }
                    };
                    let sf: SpreadingFactor = match m.get("sf").unwrap() {
                        Pmt::U32(5) => SpreadingFactor::_5,
                        Pmt::U32(6) => SpreadingFactor::_6,
                        Pmt::U32(7) => SpreadingFactor::_7,
                        Pmt::U32(8) => SpreadingFactor::_8,
                        Pmt::U32(9) => SpreadingFactor::_9,
                        Pmt::U32(10) => SpreadingFactor::_10,
                        Pmt::U32(11) => SpreadingFactor::_11,
                        Pmt::U32(12) => SpreadingFactor::_12,
                        _ => panic!(
                            "invalid Spreading Factor in received msg: {:?}",
                            m.get("sf")
                        ),
                    };
                    let bw: Bandwidth = match m.get("bw").unwrap() {
                        Pmt::U32(125) => Bandwidth::_125KHz,
                        Pmt::U32(250) => Bandwidth::_250KHz,
                        Pmt::U32(500) => Bandwidth::_500KHz,
                        _ => panic!("invalid Bandwidth in received msg: {:?}", m.get("bw")),
                    };
                    let freq: f64 = match m.get("freq").unwrap() {
                        Pmt::F64(f) => *f,
                        _ => panic!("invalid Frequency in received msg: {:?}", m.get("freq")),
                    };
                    let stat = match m.get("has_crc").unwrap() {
                        Pmt::Bool(true) => CRC::OK,
                        Pmt::Bool(false) => CRC::Disabled,
                        _ => panic!("invalid has_crc in received msg: {:?}", m.get("has_crc")),
                    };
                    let mut cfo: f32 = 0.0;
                    if let Some(Pmt::Isize(cfo_int)) = m.get("cfo_int") {
                        cfo += *cfo_int as f32
                            * (bw.hz() as f32 / (1 << Into::<u32>::into(sf)) as f32);
                    }
                    if let Some(Pmt::F64(cfo_frac)) = m.get("cfo_frac") {
                        cfo += *cfo_frac as f32;
                    }
                    let received_signal_info = match m.get("snr").unwrap() {
                        Pmt::F64(snr) => RSig {
                            ant: 0,
                            chan: 0,
                            rssic: 0,
                            rssis: None,
                            lsnr: *snr as f32,
                            etime: None, // TODO how 'encrypted'?
                            foff: Some(cfo as i64),
                            ftstat: None,
                            ftver: None,
                            ftdelta: None,
                        },
                        _ => panic!("invalid SNR in received msg: {:?}", m.get("snr")),
                    };
                    let timestamp_header_start_unix_time_nanos =
                        if let Some(Pmt::U64(tmp)) = m.get("timestamp") {
                            *tmp
                        } else {
                            panic!(
                                "invalid timestamp in received msg: {:?}",
                                m.get("timestamp")
                            )
                        };
                    let timestamp = Duration::from_nanos(timestamp_header_start_unix_time_nanos);
                    let timestamp = SystemTime::UNIX_EPOCH.checked_add(timestamp).unwrap();
                    let tmst = (timestamp_header_start_unix_time_nanos / 1000) as u32; // overflowing 32bit counter with microsecond resolution, only meaningful w.r.t. this receiver instance / gateway
                    let utc_time: DateTime<Utc> = timestamp.into();
                    let time = Some(utc_time.format("%Y%m%dT%H%M%S%.6fZ").to_string());
                    let gps_time_reference = chrono::NaiveDate::from_ymd_opt(1980, 1, 6)
                        .unwrap()
                        .and_hms_opt(0, 0, 0)
                        .unwrap()
                        .and_utc();
                    let tmms = Some(
                        utc_time
                            .signed_duration_since(gps_time_reference)
                            .num_milliseconds() as u64,
                    );
                    let rxpk = RxPk::V2(RxPkV2 {
                        aesk: 0,
                        brd: 0,
                        codr: Some(codr),
                        data: payload.clone(),
                        datr: DataRate::new(sf, bw),
                        freq,
                        jver: 2,
                        modu: "LORA".to_owned(),
                        rsig: vec![received_signal_info],
                        size: payload.len() as u64,
                        stat,
                        tmst,
                        delayed: None,
                        tmms,
                        time,
                    });
                    let packet = Packet::from_rxpk(self.mac_addr, rxpk);
                    self.uplink_sender.send(packet).await.unwrap();
                } else {
                    panic!(
                        "invalid payload in received msg. Expected Pmt::Blob, got {:?}",
                        m.get("payload")
                    )
                }
            }
            _ => panic!("invalid message type to msg in port. Expected Pmt::MapStrPmt, got {p}"),
        }
        Ok(Pmt::Ok)
    }
}
