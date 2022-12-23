//! An ADS-B receiver
use serde::Serialize;
use serde_with::{serde_as, DisplayFromStr};
use std::collections::HashMap;
use std::time::SystemTime;

/// Demodulator sample rate
pub const DEMOD_SAMPLE_RATE: usize = 4000000;
/// Number of samples per PPM half-symbol at `DEMOD_SAMPLE_RATE`.
pub const N_SAMPLES_PER_HALF_SYM: usize = DEMOD_SAMPLE_RATE / 2000000;
/// Taps representing a HIGH symbol
pub const SYMBOL_ONE_TAPS: [f32; 2 * N_SAMPLES_PER_HALF_SYM] = [1.0, 1.0, -1.0, -1.0];
/// Taps representing a LOW symbol
pub const SYMBOL_ZERO_TAPS: [f32; 2 * N_SAMPLES_PER_HALF_SYM] = [-1.0, -1.0, 1.0, 1.0];

mod preamble_detector;
pub use preamble_detector::PreambleDetector;

mod demodulator;
pub use demodulator::DemodPacket;
pub use demodulator::Demodulator;

mod decoder;
pub use decoder::AdsbPacket;
pub use decoder::Decoder;

mod tracker;
pub use tracker::Tracker;

type AdsbIcao = adsb_deku::ICAO;
type AdsbIdentification = adsb_deku::adsb::Identification;
type AdsbPosition = adsb_deku::Altitude;
type AdsbVelocity = adsb_deku::adsb::AirborneVelocity;

/// Represents the position of an aircraft.
#[derive(Serialize, Clone, Debug)]
pub struct AircraftPosition {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: Option<u16>,
    pub type_code: u8,
}

/// Represents the source of the vertical rate.
#[derive(Serialize, Clone, Debug)]
pub enum AircraftVerticalRateSource {
    BarometricPressureAltitude,
    GeometricAltitude,
}

/// Represents the velocity of an aircraft.
#[derive(Serialize, Clone, Debug)]
pub struct AircraftVelocity {
    pub heading: f64,
    pub ground_speed: f64,
    pub vertical_rate: i16,
    pub vertical_rate_source: AircraftVerticalRateSource,
}

/// Represents a received position of an aircraft.
#[derive(Serialize, Clone, Debug)]
pub struct AircraftPositionRecord {
    pub position: AircraftPosition,
    pub time: SystemTime,
}

/// Represents a received velocity of an aircraft.
#[derive(Serialize, Clone, Debug)]
pub struct AircraftVelocityRecord {
    pub velocity: AircraftVelocity,
    pub time: SystemTime,
}

/// Represents a received CPR frame.
#[derive(Clone, Debug)]
pub struct CprFrameRecord {
    pub cpr_frame: AdsbPosition,
    pub time: SystemTime,
}

/// Represents a summary of the received information about an aircraft.
#[serde_as]
#[derive(Serialize, Clone, Debug)]
pub struct AircraftRecord {
    #[serde_as(as = "DisplayFromStr")]
    pub icao: AdsbIcao,
    pub callsign: Option<String>,
    pub emitter_category: Option<u8>,
    pub positions: Vec<AircraftPositionRecord>,
    pub velocities: Vec<AircraftVelocityRecord>,
    #[serde(skip)]
    pub last_cpr_even: Option<CprFrameRecord>,
    #[serde(skip)]
    pub last_cpr_odd: Option<CprFrameRecord>,
    pub last_seen: SystemTime,
}

/// Represents a collection of received aircrafts.
#[serde_as]
#[derive(Serialize, Clone, Debug)]
pub struct AircraftRegister {
    #[serde_as(as = "HashMap<DisplayFromStr, _>")]
    register: HashMap<AdsbIcao, AircraftRecord>,
}
