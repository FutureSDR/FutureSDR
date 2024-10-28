use futuresdr::num_complex::Complex32;
use futuresdr::tracing::warn;

use crate::utils::build_ref_chirps;
use crate::utils::build_upchirp;
use crate::utils::expand_sync_word;
use crate::utils::LEGACY_SF_5_6;

pub struct Modulator {
    spreading_factor: usize,
    oversampling: usize,
    sync_words: Vec<usize>,
    preamble_len: usize,
    pad_front: usize,
    pad_tail: usize,
}

impl Modulator {
    pub fn new(
        spreading_factor: usize,
        oversampling: usize,
        sync_words: Vec<usize>,
        mut preamble_len: usize,
        pad: usize,
    ) -> Self {
        if preamble_len < 5 {
            warn!("Preamble length should be at least 5!");
            preamble_len = 5;
        }
        let sync_words = expand_sync_word(sync_words);
        Modulator {
            spreading_factor,
            oversampling,
            sync_words,
            preamble_len,
            pad_front: pad,
            pad_tail: pad,
        }
    }

    pub fn modulate(&self, frame: Vec<u16>) -> Vec<Complex32> {
        let (upchirp, downchirp) = build_ref_chirps(self.spreading_factor, self.oversampling);
        let mut preamb_samp_cnt = 0;

        let mut out = vec![Complex32::new(0.0, 0.0); self.pad_front];

        loop {
            // output preamble part
            if preamb_samp_cnt
                < self.preamble_len
                    + 5
                    + if self.spreading_factor < 7 && !LEGACY_SF_5_6 {
                        2
                    } else {
                        0
                    }
            {
                // upchirps
                if preamb_samp_cnt < self.preamble_len {
                    out.extend_from_slice(&upchirp);
                // sync words
                } else if preamb_samp_cnt == self.preamble_len {
                    let sync_upchirp =
                        build_upchirp(self.sync_words[0], self.spreading_factor, self.oversampling);
                    out.extend_from_slice(&sync_upchirp);
                } else if preamb_samp_cnt == self.preamble_len + 1 {
                    let sync_upchirp =
                        build_upchirp(self.sync_words[1], self.spreading_factor, self.oversampling);
                    out.extend_from_slice(&sync_upchirp);
                // 2.25 downchirps
                } else if preamb_samp_cnt < self.preamble_len + 4 {
                    out.extend_from_slice(&downchirp);
                } else if preamb_samp_cnt == self.preamble_len + 4 {
                    out.extend_from_slice(&downchirp[..downchirp.len() / 4]);
                } else if self.spreading_factor < 7
                    && !LEGACY_SF_5_6
                    && preamb_samp_cnt < self.preamble_len + 7
                {
                    out.extend_from_slice(&upchirp);
                }
                preamb_samp_cnt += 1;
            } else {
                break;
            }
        }

        for sample in frame {
            let data_upchirp =
                build_upchirp(sample as usize, self.spreading_factor, self.oversampling);
            out.extend_from_slice(&data_upchirp);
        }

        out.extend_from_slice(&vec![Complex32::new(0.0, 0.0); self.pad_tail]);
        out
    }
}
