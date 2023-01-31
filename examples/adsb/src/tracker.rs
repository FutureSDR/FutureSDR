use crate::decoder::{AdsbPacket, DecoderMetaData};
use futuresdr::anyhow::Result;
use futuresdr::async_io::Timer;
use futuresdr::async_trait::async_trait;
use futuresdr::futures::FutureExt;
use futuresdr::log::{info, warn};
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::WorkIo;

use std::cmp::Ordering;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, SystemTime};

use crate::*;

/// The duration considered to be recent when decoding CPR frames
const ADSB_TIME_RECENT: Duration = Duration::new(10, 0);

pub struct Tracker {
    /// When to prune aircraft from the register.
    prune_after: Option<Duration>,
    /// A register of the received aircrafts.
    aircraft_register: AircraftRegister,
}

impl Tracker {
    /// Creates a new tracker without pruning.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Block {
        Tracker::new_with_optional_args(None)
    }

    pub fn with_pruning(after: Duration) -> Block {
        Tracker::new_with_optional_args(Some(after))
    }

    fn new_with_optional_args(prune_after: Option<Duration>) -> Block {
        let aircraft_register = AircraftRegister {
            register: HashMap::new(),
        };
        Block::new(
            BlockMetaBuilder::new("Tracker").build(),
            StreamIoBuilder::new().build(),
            MessageIoBuilder::new()
                .add_input("in", Self::packet_received)
                .add_input("ctrl_port", Self::handle_ctrl_port)
                .build(),
            Self {
                prune_after,
                aircraft_register,
            },
        )
    }

    /// This function handles control port messages.
    fn handle_ctrl_port<'a>(
        &'a mut self,
        _mio: &'a mut MessageIo<Self>,
        _meta: &'a mut BlockMeta,
        p: Pmt,
    ) -> Pin<Box<dyn Future<Output = Result<Pmt>> + Send + 'a>> {
        async move {
            match p {
                Pmt::Null => {
                    // Reply with register
                    let json = serde_json::to_string(&self.aircraft_register).unwrap();
                    Ok(Pmt::String(json))
                }
                x => {
                    warn!("Received unexpected PMT type: {:?}", x);
                    Ok(Pmt::Null)
                }
            }
        }
        .boxed()
    }

    /// This function handles received packets passed to the block.
    fn packet_received<'a>(
        &'a mut self,
        _mio: &'a mut MessageIo<Self>,
        _meta: &'a mut BlockMeta,
        p: Pmt,
    ) -> Pin<Box<dyn Future<Output = Result<Pmt>> + Send + 'a>> {
        async move {
            match p {
                Pmt::Any(a) => {
                    if let Some(adsb_packet) = a.downcast_ref::<AdsbPacket>() {
                        // We received a packet. Update the register.
                        info!("Received {:?}", adsb_packet);
                        if let adsb_deku::DF::ADSB(adsb) = &adsb_packet.message.df {
                            let metadata = &adsb_packet.decoder_metadata;
                            match &adsb.me {
                                adsb_deku::adsb::ME::AircraftIdentification(identification) => self
                                    .aircraft_identification_received(
                                        &adsb.icao,
                                        identification,
                                        metadata,
                                    ),
                                adsb_deku::adsb::ME::AirbornePositionBaroAltitude(altitude)
                                | adsb_deku::adsb::ME::AirbornePositionGNSSAltitude(altitude) => {
                                    self.airborne_position_received(&adsb.icao, altitude, metadata)
                                }
                                adsb_deku::adsb::ME::AirborneVelocity(velocity) => {
                                    self.airborne_velocity_received(&adsb.icao, velocity, metadata)
                                }
                                _ => (),
                            }
                        }
                    }
                }
                x => {
                    warn!("Received unexpected PMT type: {:?}", x);
                }
            }
            Ok(Pmt::Null)
        }
        .boxed()
    }

    fn update_last_seen(&mut self, icao: &AdsbIcao) {
        if let Some(rec) = self.aircraft_register.register.get_mut(icao) {
            // Update the time stamp in the register record
            rec.last_seen = SystemTime::now();
        }
    }

    fn register_aircraft(&mut self, icao: &AdsbIcao) {
        // Add an aircraft record to our register map
        let now = SystemTime::now();
        let record = AircraftRecord {
            icao: *icao,
            callsign: None,
            emitter_category: None,
            positions: Vec::new(),
            velocities: Vec::new(),
            last_cpr_even: None,
            last_cpr_odd: None,
            last_seen: now,
        };
        if self.aircraft_register.register.contains_key(icao) {
            warn!("Aircraft {} is already registered and will be reset", icao);
        }
        self.aircraft_register.register.insert(*icao, record);
    }

    fn prune_records(&mut self) {
        if let Some(prune_time) = self.prune_after {
            let now = SystemTime::now();
            self.aircraft_register
                .register
                .retain(|_, v| v.last_seen + prune_time >= now);
        }
    }

    fn aircraft_identification_received(
        &mut self,
        icao: &AdsbIcao,
        identification: &AdsbIdentification,
        _metadata: &DecoderMetaData,
    ) {
        if !self.aircraft_register.register.contains_key(icao) {
            self.register_aircraft(icao);
        }
        let rec = self.aircraft_register.register.get_mut(icao).unwrap();
        rec.callsign = Some(identification.cn.clone());
        rec.emitter_category = Some(identification.ca);
        self.update_last_seen(icao);
    }

    fn airborne_position_received(
        &mut self,
        icao: &AdsbIcao,
        altitude: &AdsbPosition,
        _metadata: &DecoderMetaData,
    ) {
        if !self.aircraft_register.register.contains_key(icao) {
            self.register_aircraft(icao);
        }
        let now = SystemTime::now();
        let rec = self.aircraft_register.register.get_mut(icao).unwrap();

        // Update record
        let cpr_rec = CprFrameRecord {
            cpr_frame: *altitude,
            time: now,
        };
        match altitude.odd_flag {
            adsb_deku::CPRFormat::Even => rec.last_cpr_even = Some(cpr_rec),
            adsb_deku::CPRFormat::Odd => rec.last_cpr_odd = Some(cpr_rec),
        }

        // Check if we can calculate the position. This requires both an odd
        // and an even frame.
        // Make rec immutable
        let rec = self.aircraft_register.register.get(icao).unwrap();
        if rec.last_cpr_even.is_some() && rec.last_cpr_odd.is_some() {
            // The frames must be recent
            let even_cpr_rec = rec.last_cpr_even.as_ref().unwrap();
            let odd_cpr_rec = rec.last_cpr_odd.as_ref().unwrap();
            if even_cpr_rec.time < now + ADSB_TIME_RECENT
                && odd_cpr_rec.time < now + ADSB_TIME_RECENT
            {
                // The CPR frames must be orderd by time
                let (cpr1, cpr2) = match even_cpr_rec.time.cmp(&odd_cpr_rec.time) {
                    Ordering::Less => (even_cpr_rec, odd_cpr_rec),
                    Ordering::Greater | Ordering::Equal => (odd_cpr_rec, even_cpr_rec),
                };
                if let Some(pos) = adsb_deku::cpr::get_position((&cpr1.cpr_frame, &cpr2.cpr_frame))
                {
                    // We got a position!
                    // Add it to the record
                    let new_pos = AircraftPosition {
                        latitude: pos.latitude,
                        longitude: pos.longitude,
                        altitude: altitude.alt,
                        type_code: altitude.tc,
                    };
                    let new_rec = AircraftPositionRecord {
                        position: new_pos,
                        time: now,
                    };
                    let rec = self.aircraft_register.register.get_mut(icao).unwrap();
                    rec.positions.push(new_rec);
                }
            }
        }
        self.update_last_seen(icao);
    }

    fn airborne_velocity_received(
        &mut self,
        icao: &AdsbIcao,
        velocity: &AdsbVelocity,
        _metadata: &DecoderMetaData,
    ) {
        if !self.aircraft_register.register.contains_key(icao) {
            self.register_aircraft(icao);
        }
        let now = SystemTime::now();
        // Calculate the velocity
        if let Some((heading, ground_speed, vertical_rate)) = velocity.calculate() {
            // Add it to the record
            let new_velocity = AircraftVelocity {
                heading: heading as f64,
                ground_speed,
                vertical_rate,
                vertical_rate_source: match velocity.vrate_src {
                    adsb_deku::adsb::VerticalRateSource::BarometricPressureAltitude => {
                        AircraftVerticalRateSource::BarometricPressureAltitude
                    }
                    adsb_deku::adsb::VerticalRateSource::GeometricAltitude => {
                        AircraftVerticalRateSource::GeometricAltitude
                    }
                },
            };
            let new_record = AircraftVelocityRecord {
                velocity: new_velocity,
                time: now,
            };
            let rec = self.aircraft_register.register.get_mut(icao).unwrap();
            rec.velocities.push(new_record);
        }
        self.update_last_seen(icao);
    }
}

#[async_trait]
impl Kernel for Tracker {
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        // Set up pruning timer.
        // To keep things simple, we just run the prune
        // function every second, although this means that any
        // item may remain for sec. longer than the prune duration.
        if self.prune_after.is_some() {
            Timer::after(Duration::from_millis(1000)).await;
            self.prune_records();
        }

        Ok(())
    }
}
