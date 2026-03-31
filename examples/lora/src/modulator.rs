use futuresdr::num_complex::Complex32;
use futuresdr::prelude::Result;
use futuresdr::tracing::warn;

use crate::utils::SpreadingFactor;
use crate::utils::SynchWord;
use crate::utils::build_upchirp_phase_coherent;
use crate::utils::samples_from_phase_diff;

pub struct Modulator {
    spreading_factor: SpreadingFactor,
    oversampling: usize,
    sync_word: [usize; 2],
    preamble_len: usize,
    pad_front: usize,
    pad_tail: usize,
}

impl Modulator {
    pub fn new(
        spreading_factor: SpreadingFactor,
        oversampling: usize,
        sync_word: SynchWord,
        preamble_len: usize,
        pad: usize,
    ) -> Result<Self> {
        if preamble_len < 5 {
            warn!("Preamble length should be at least 5!"); // as specified in semtech datasheets
        }
        Ok(Modulator {
            spreading_factor,
            oversampling,
            sync_word: sync_word.verify_and_expand(spreading_factor)?,
            preamble_len,
            pad_front: pad,
            pad_tail: pad,
        })
    }

    pub fn set_synch_word(&mut self, synch_word: SynchWord) -> Result<()> {
        let sync_word_expanded = synch_word.verify_and_expand(self.spreading_factor)?;
        self.sync_word = sync_word_expanded;
        Ok(())
    }

    pub fn modulate(&self, frame: Vec<u16>) -> Vec<Complex32> {
        let mut preamb_samp_cnt = 0;

        let mut phase_increments = vec![0.0; self.pad_front];

        loop {
            // output preamble part
            if preamb_samp_cnt
                < self.preamble_len
                    + 5
                    + if self.spreading_factor < SpreadingFactor::SF7 {
                        2
                    } else {
                        0
                    }
            {
                // upchirps
                if preamb_samp_cnt < self.preamble_len {
                    let upchirp = build_upchirp_phase_coherent(
                        0,
                        self.spreading_factor.into(),
                        self.oversampling,
                        true,
                        None,
                        false,
                    );
                    phase_increments.extend_from_slice(&upchirp);
                // sync words
                } else if preamb_samp_cnt == self.preamble_len {
                    let sync_upchirp = build_upchirp_phase_coherent(
                        self.sync_word[0],
                        self.spreading_factor.into(),
                        self.oversampling,
                        true,
                        None,
                        false,
                    );
                    phase_increments.extend_from_slice(&sync_upchirp);
                } else if preamb_samp_cnt == self.preamble_len + 1 {
                    let sync_upchirp = build_upchirp_phase_coherent(
                        self.sync_word[1],
                        self.spreading_factor.into(),
                        self.oversampling,
                        true,
                        None,
                        false,
                    );
                    phase_increments.extend_from_slice(&sync_upchirp);
                // 2.25 downchirps
                } else if preamb_samp_cnt < self.preamble_len + 4 {
                    let downchirp = build_upchirp_phase_coherent(
                        0,
                        self.spreading_factor.into(),
                        self.oversampling,
                        false,
                        None,
                        false,
                    );
                    phase_increments.extend_from_slice(&downchirp);
                } else if preamb_samp_cnt == self.preamble_len + 4 {
                    let downchirp = build_upchirp_phase_coherent(
                        0,
                        self.spreading_factor.into(),
                        self.oversampling,
                        false,
                        Some(
                            (self.spreading_factor.samples_per_symbol() * self.oversampling) / 4
                                - self.oversampling, // quarter down is one baseband sample shorter than a quarter symbol
                        ),
                        false,
                    );
                    phase_increments.extend_from_slice(&downchirp);
                } else if self.spreading_factor < SpreadingFactor::SF7
                    && preamb_samp_cnt < self.preamble_len + 7
                {
                    let upchirp = build_upchirp_phase_coherent(
                        0,
                        self.spreading_factor.into(),
                        self.oversampling,
                        true,
                        None,
                        false,
                    );
                    phase_increments.extend_from_slice(&upchirp);
                }
                preamb_samp_cnt += 1;
            } else {
                break;
            }
        }

        for sample in frame {
            let data_upchirp = build_upchirp_phase_coherent(
                sample as usize,
                self.spreading_factor.into(),
                self.oversampling,
                true,
                None,
                true,
            );
            phase_increments.extend_from_slice(&data_upchirp);
        }

        phase_increments.extend_from_slice(&vec![0.0; self.pad_tail]);

        samples_from_phase_diff(&phase_increments)
    }
}
