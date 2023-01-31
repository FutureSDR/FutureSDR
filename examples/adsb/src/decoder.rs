use crate::DemodPacket;
use adsb_deku::deku::DekuContainerRead;
use futuresdr::anyhow::{bail, Result};
use futuresdr::async_trait::async_trait;
use futuresdr::futures::FutureExt;
use futuresdr::log::{debug, info, warn};
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::Pmt;
use serde::Serialize;
use std::future::Future;
use std::pin::Pin;
use std::time::SystemTime;

fn bin_to_u64(s: &[u8]) -> u64 {
    s.iter().fold(0, |acc, &b| (acc << 1) + b as u64)
}

#[derive(Debug, Clone, Serialize)]
pub struct DecoderMetaData {
    pub preamble_index: u64,
    pub preamble_correlation: f32,
    pub crc_passed: bool,
    pub timestamp: SystemTime,
}

#[derive(Debug, Clone)]
pub struct AdsbPacket {
    pub message: adsb_deku::Frame,
    pub decoder_metadata: DecoderMetaData,
}

pub struct Decoder {
    forward_failed_crc: bool,
    n_crc_ok: u64,
    n_crc_fail: u64,
}

impl Decoder {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(forward_failed_crc: bool) -> Block {
        Block::new(
            BlockMetaBuilder::new("Decoder").build(),
            StreamIoBuilder::new().build(),
            MessageIoBuilder::new()
                .add_input("in", Self::packet_received)
                .add_output("out")
                .build(),
            Self {
                forward_failed_crc,
                n_crc_ok: 0,
                n_crc_fail: 0,
            },
        )
    }

    /// Checks if the CRC is valid
    fn check_crc(&self, bits: &[u8]) -> bool {
        let mut bits = bits.to_vec();
        const GENERATOR_POLY: [u8; 25] = [
            1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1,
        ];
        for i in 0..bits.len() - (GENERATOR_POLY.len() - 1) {
            if bits[i] == 1 {
                for j in 0..GENERATOR_POLY.len() {
                    bits[i + j] ^= GENERATOR_POLY[j];
                }
            }
        }
        bits[(bits.len() - (GENERATOR_POLY.len() - 1))..bits.len()]
            .iter()
            .sum::<u8>()
            == 0
    }

    /// Decodes demodulated packet bits
    fn decode_packet(
        &self,
        packet: &DemodPacket,
        crc_passed: bool,
        timestamp: SystemTime,
    ) -> Result<AdsbPacket> {
        let decoder_metadata = DecoderMetaData {
            preamble_index: packet.preamble_index,
            preamble_correlation: packet.preamble_correlation,
            crc_passed,
            timestamp,
        };
        // Decode downlink format
        let bytes: Vec<u8> = (0..packet.bits.len())
            .step_by(8)
            .map(|i| bin_to_u64(&packet.bits[i..i + 8]) as u8)
            .collect();
        match adsb_deku::Frame::from_bytes((&bytes, 0)) {
            Ok((_, message)) => {
                let packet = AdsbPacket {
                    message,
                    decoder_metadata,
                };
                Ok(packet)
            }
            Err(_) => bail!("adsb_deku could not parse packet"),
        }
    }

    fn packet_received<'a>(
        &'a mut self,
        mio: &'a mut MessageIo<Self>,
        _meta: &'a mut BlockMeta,
        p: Pmt,
    ) -> Pin<Box<dyn Future<Output = Result<Pmt>> + Send + 'a>> {
        async move {
            match p {
                Pmt::Any(a) => {
                    if let Some(pkt) = a.downcast_ref::<DemodPacket>()
                    {
                        // Validate the CRC before we start decoding
                        let crc_passed = self.check_crc(&pkt.bits);
                        if crc_passed {
                            self.n_crc_ok += 1;
                            debug!(
                                "Decoded packet with CRC OK (index: {}, preamble correlation: {}, data: {:?})",
                                pkt.preamble_index, pkt.preamble_correlation, pkt.bits
                            );
                        } else {
                            self.n_crc_fail += 1;
                            debug!(
                                "Decoded packet with CRC error (index: {}, preamble correlation: {}, data: {:?})",
                                pkt.preamble_index, pkt.preamble_correlation, pkt.bits
                            );
                        }

                        if crc_passed || self.forward_failed_crc {
                            match self.decode_packet(
                                pkt,
                                crc_passed,
                                SystemTime::now(),
                            ) {
                                Ok(decoded_packet) => {
                                    mio.output_mut(0).post(Pmt::Any(Box::new(decoded_packet))).await
                                }
                                _ => info!("Could not decode packet despite valid CRC"),
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
}

#[async_trait]
impl Kernel for Decoder {}
