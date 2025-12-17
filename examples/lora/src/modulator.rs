use futuresdr::num_complex::Complex32;
use futuresdr::tracing::warn;

use crate::utils::SpreadingFactor;
use crate::utils::build_upchirp_phase_coherent;
use crate::utils::expand_sync_word;

pub struct Modulator {
    spreading_factor: SpreadingFactor,
    oversampling: usize,
    sync_words: Vec<usize>,
    preamble_len: usize,
    pad_front: usize,
    pad_tail: usize,
}

impl Modulator {
    pub fn new(
        spreading_factor: SpreadingFactor,
        oversampling: usize,
        sync_words: Vec<usize>,
        preamble_len: usize,
        pad: usize,
    ) -> Self {
        if preamble_len < 5 {
            warn!("Preamble length should be at least 5!"); // TODO
        }
        let sync_words_expanded = expand_sync_word(sync_words.clone());
        if sync_words[0] != 0x12 && spreading_factor < SpreadingFactor::SF7 {
            warn!(
                "LoRa Modulator: selecting sync word other than 0x12 for SF < 7 will likely not work with commercial receivers.\n\tSee e.g. https://github.com/Lora-net/sx1302_hal/issues/124#issuecomment-2173450337"
            );
        }
        for sync_word_symbol in sync_words_expanded.iter() {
            if *sync_word_symbol >= 1 << Into::<usize>::into(spreading_factor) {
                panic!(
                    "LoRa Modulator: can not encode chosen sync word with the given spreading factor: symbol space too small.\n\ttried to encode sync word '{}' (symbol values [{}, {}]), with {}, which only supports symbols within [0; {}].",
                    sync_words[0],
                    sync_words_expanded[0],
                    sync_words_expanded[1],
                    spreading_factor,
                    1 << Into::<usize>::into(spreading_factor)
                );
            }
        }
        Modulator {
            spreading_factor,
            oversampling,
            sync_words: sync_words_expanded,
            preamble_len,
            pad_front: pad,
            pad_tail: pad,
        }
    }

    fn samples_from_phase_diff(&self, phase_increments: &[f32]) -> Vec<Complex32> {
        let mut last_phase = 0.0;
        phase_increments
            .iter()
            .map(|p_i| {
                let tmp = Complex32::new(1.0, 0.0) * Complex32::from_polar(1., last_phase + *p_i);
                last_phase += *p_i;
                tmp
            })
            .collect()
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
                        self.sync_words[0],
                        self.spreading_factor.into(),
                        self.oversampling,
                        true,
                        None,
                        false,
                    );
                    phase_increments.extend_from_slice(&sync_upchirp);
                } else if preamb_samp_cnt == self.preamble_len + 1 {
                    let sync_upchirp = build_upchirp_phase_coherent(
                        self.sync_words[1],
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

        self.samples_from_phase_diff(&phase_increments)
    }
}
