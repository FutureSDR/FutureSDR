use crate::utils::CodeRate;
use crate::utils::SpreadingFactor;
use crate::utils::WHITENING_SEQ;
use crate::utils::bool2int;
use crate::utils::int2bool;
use crate::utils::my_modulo;

pub struct Encoder {
    code_rate: CodeRate,
    pub spreading_factor: SpreadingFactor,
    has_crc: bool,
    low_data_rate: bool,
    implicit_header: bool,
}

impl Encoder {
    pub fn new(
        code_rate: CodeRate,
        spreading_factor: SpreadingFactor,
        has_crc: bool,
        low_data_rate: bool,
        implicit_header: bool,
    ) -> Encoder {
        Encoder {
            code_rate,
            spreading_factor,
            has_crc,
            low_data_rate,
            implicit_header,
        }
    }

    pub fn encode(&self, payload: Vec<u8>) -> Vec<u16> {
        let frame = Self::whitening(&payload);
        let frame = if self.implicit_header {
            frame
        } else {
            Self::header(frame, self.code_rate, self.has_crc)
        };
        let frame = if self.has_crc {
            Self::crc(frame, &payload)
        } else {
            frame
        };
        let frame = Self::hamming_encode(frame, self.code_rate, self.spreading_factor);
        let frame = Self::interleave(
            frame,
            self.code_rate,
            self.spreading_factor,
            self.low_data_rate,
        );
        Self::gray_demap(frame, self.spreading_factor)
    }

    fn whitening(payload: &[u8]) -> Vec<u8> {
        let mut out = vec![0; payload.len() * 2];
        for i in 0..payload.len() {
            out[2 * i] = (payload[i] ^ WHITENING_SEQ[i]) & 0x0F;
            out[2 * i + 1] = (payload[i] ^ WHITENING_SEQ[i]) >> 4;
        }
        out
    }

    fn header(whitened: Vec<u8>, code_rate: CodeRate, has_crc: bool) -> Vec<u8> {
        let payload_len = whitened.len() / 2;
        let mut out = vec![0; whitened.len() + 5];

        out[0] = (payload_len >> 4) as u8;
        out[1] = (payload_len & 0x0F) as u8;
        // coding rate and has_crc
        out[2] = (Into::<u8>::into(code_rate) << 1) | (has_crc as u8);
        // header checksum
        let c4 = (out[0] & 0b1000) >> 3
            ^ (out[0] & 0b0100) >> 2
            ^ (out[0] & 0b0010) >> 1
            ^ (out[0] & 0b0001);
        let c3 = (out[0] & 0b1000) >> 3
            ^ (out[1] & 0b1000) >> 3
            ^ (out[1] & 0b0100) >> 2
            ^ (out[1] & 0b0010) >> 1
            ^ (out[2] & 0b0001);
        let c2 = (out[0] & 0b0100) >> 2
            ^ (out[1] & 0b1000) >> 3
            ^ (out[1] & 0b0001)
            ^ (out[2] & 0b1000) >> 3
            ^ (out[2] & 0b0010) >> 1;
        let c1 = (out[0] & 0b0010) >> 1
            ^ (out[1] & 0b0100) >> 2
            ^ (out[1] & 0b0001)
            ^ (out[2] & 0b0100) >> 2
            ^ (out[2] & 0b0010) >> 1
            ^ (out[2] & 0b0001);
        let c0 = (out[0] & 0b0001)
            ^ (out[1] & 0b0010) >> 1
            ^ (out[2] & 0b1000) >> 3
            ^ (out[2] & 0b0100) >> 2
            ^ (out[2] & 0b0010) >> 1
            ^ (out[2] & 0b0001);
        out[3] = c4;
        out[4] = c3 << 3 | c2 << 2 | c1 << 1 | c0;
        out[5..].clone_from_slice(&whitened);
        out
    }

    fn crc16(crc_value_in: u16, new_byte_tmp: u8) -> u16 {
        let mut crc_value = crc_value_in;
        let mut new_byte = new_byte_tmp as u16;
        for _i in 0..8 {
            if ((crc_value & 0x8000) >> 8) ^ (new_byte & 0x80) != 0 {
                crc_value = (crc_value << 1) ^ 0x1021;
            } else {
                crc_value <<= 1;
            }
            new_byte <<= 1;
        }
        crc_value
    }

    fn crc(mut frame: Vec<u8>, payload: &[u8]) -> Vec<u8> {
        let mut crc: u16 = 0x0000;
        let payload_len = payload.len();
        for i in payload.iter().take(payload_len - 2) {
            crc = Self::crc16(crc, *i);
        }
        // XOR the CRC with the last 2 data bytes
        crc = crc ^ (payload[payload_len - 1] as u16) ^ ((payload[payload_len - 2] as u16) << 8);
        frame.push((crc & 0x000F) as u8);
        frame.push(((crc & 0x00F0) >> 4) as u8);
        frame.push(((crc & 0x0F00) >> 8) as u8);
        frame.push(((crc & 0xF000) >> 12) as u8);
        frame
    }

    fn hamming_encode(
        frame: Vec<u8>,
        code_rate: CodeRate,
        spreading_factor: SpreadingFactor,
    ) -> Vec<u8> {
        let mut out = vec![0; frame.len()];
        for i in 0..frame.len() {
            let cr_app: u8 = if i
                < (Into::<usize>::into(spreading_factor)
                    - if spreading_factor < SpreadingFactor::SF7 {
                        0
                    } else {
                        2
                    })
            //for sf<7 we don'tuse the ldro
            {
                4
            } else {
                code_rate.into()
            };
            let data_bin = int2bool(frame[i] as u16, 4);
            if cr_app != 1 {
                // hamming parity bits
                let p0 = (data_bin[3] ^ data_bin[2] ^ data_bin[1]) as u8;
                let p1 = (data_bin[2] ^ data_bin[1] ^ data_bin[0]) as u8;
                let p2 = (data_bin[3] ^ data_bin[2] ^ data_bin[0]) as u8;
                let p3 = (data_bin[3] ^ data_bin[1] ^ data_bin[0]) as u8;
                // put the data LSB first and append the parity bits
                out[i] = ((data_bin[3] as u8) << 7
                    | (data_bin[2] as u8) << 6
                    | (data_bin[1] as u8) << 5
                    | (data_bin[0] as u8) << 4
                    | p0 << 3
                    | p1 << 2
                    | p2 << 1
                    | p3)
                    >> (4 - cr_app);
            } else {
                // coding rate = 4/5 -> add parity bit
                let p4 = (data_bin[0] ^ data_bin[1] ^ data_bin[2] ^ data_bin[3]) as u8;
                out[i] = (data_bin[3] as u8) << 4
                    | (data_bin[2] as u8) << 3
                    | (data_bin[1] as u8) << 2
                    | (data_bin[0] as u8) << 1
                    | p4;
            }
        }
        out
    }

    fn interleave(
        mut frame: Vec<u8>,
        code_rate: CodeRate,
        spreading_factor: SpreadingFactor,
        low_data_rate: bool,
    ) -> Vec<u16> {
        let mut cnt: usize = 0;
        let mut out = Vec::new();

        loop {
            // handle the first interleaved block special case
            let (cw_len, use_ldro): (u8, bool) = if spreading_factor >= SpreadingFactor::SF7 {
                (
                    4 + if cnt < Into::<usize>::into(spreading_factor) - 2 {
                        4
                    } else {
                        code_rate.into()
                    },
                    cnt < Into::<usize>::into(spreading_factor) - 2 || low_data_rate, // header or ldro activated for payload
                )
            } else
            //sf == 5 or sf ==6 don't use LDRO in header
            {
                (
                    4 + if cnt < spreading_factor.into() {
                        4
                    } else {
                        code_rate.into()
                    },
                    cnt >= spreading_factor.into() && low_data_rate, // not header and ldro activated for payload
                )
            };
            let sf_app: usize = if use_ldro {
                Into::<usize>::into(spreading_factor).saturating_sub(2)
            } else {
                Into::<usize>::into(spreading_factor)
            };

            let curr;
            if frame.len() <= sf_app {
                curr = frame;
                frame = Vec::new();
            } else {
                let t = frame.split_off(sf_app);
                curr = frame;
                frame = t;
            }

            let init_bit: Vec<bool> = vec![false; spreading_factor.into()];
            let mut inter_bin: Vec<Vec<bool>> = vec![init_bit; cw_len.into()];

            // convert to input codewords to binary vector of vector
            let cw_bin: Vec<Vec<bool>> = curr
                .iter()
                .chain(vec![0_u8; sf_app - curr.len()].iter())
                .map(|x| int2bool(*x as u16, cw_len.into()))
                .collect();

            cnt += sf_app;

            let mut tmp = vec![0; cw_len.into()];
            // do the actual interleaving
            for i in 0..cw_len as usize {
                for j in 0..sf_app {
                    inter_bin[i][j] = cw_bin[my_modulo(i as isize - j as isize - 1, sf_app)][i];
                }
                // for the first block, add a parity bit and a zero in the end of the symbol (reduced rate)
                if use_ldro {
                    inter_bin[i][sf_app] = inter_bin[i]
                        .iter()
                        .fold(0, |acc, e| acc + if *e { 1 } else { 0 })
                        % 2
                        != 0;
                }
                tmp[i] = bool2int(&inter_bin[i]);
            }

            out.extend_from_slice(&tmp);
            if frame.is_empty() {
                break;
            }
        }

        out
    }

    fn gray_demap(frame: Vec<u16>, spreading_factor: SpreadingFactor) -> Vec<u16> {
        let mut out = vec![0; frame.len()];

        for i in 0..frame.len() {
            out[i] = frame[i];
            for j in 1..Into::<usize>::into(spreading_factor) {
                out[i] ^= frame[i] >> j as u16;
            }
            out[i] = my_modulo(
                (out[i] + 1) as isize,
                1 << Into::<usize>::into(spreading_factor),
            ) as u16;
        }
        out
    }
}
