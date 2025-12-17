use futuresdr::prelude::*;
use futuresdr::runtime::buffer::Tags;
use rustfft::Fft;
use rustfft::FftDirection;
use rustfft::FftPlanner;
use std::collections::HashMap;
use std::f32::consts::PI;
use std::str::FromStr;
use std::sync::Arc;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use crate::utils::*;

#[derive(Debug, Copy, Clone, PartialEq)]
enum DecoderState {
    Detect,
    Sync,
    SfoCompensation,
}
#[repr(usize)]
#[derive(Debug, Copy, Clone, PartialEq)]
enum SyncState {
    NetId1 = 0,
    NetId2 = 1,
    Downchirp1 = 2,
    Downchirp2 = 3,
    QuarterDown = 4,
    Synced(usize),
}
impl From<usize> for SyncState {
    fn from(orig: usize) -> Self {
        match orig {
            0_usize => SyncState::NetId1,
            1_usize => SyncState::NetId2,
            2_usize => SyncState::Downchirp1,
            3_usize => SyncState::Downchirp2,
            4_usize => SyncState::QuarterDown,
            _ => SyncState::Synced(orig),
        }
    }
}
impl From<SyncState> for usize {
    fn from(orig: SyncState) -> Self {
        match orig {
            SyncState::NetId1 => 0_usize,
            SyncState::NetId2 => 1_usize,
            SyncState::Downchirp1 => 2_usize,
            SyncState::Downchirp2 => 3_usize,
            SyncState::QuarterDown => 4_usize,
            SyncState::Synced(value) => value,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
enum NetIdCachingPolicy {
    None,
    Seen,         // cache any syntactically valid NetID encountered in a detected preamble
    HeaderCrcOk,  // default strategy; if header has no CRC, behaves like None
    PayloadCrcOk, // only cache after payload has been decoded successfully, if payload has no CRC, behaves like None
}

impl FromStr for NetIdCachingPolicy {
    type Err = ();
    fn from_str(input: &str) -> Result<NetIdCachingPolicy, Self::Err> {
        match input {
            "none" => Ok(NetIdCachingPolicy::None),
            "seen" => Ok(NetIdCachingPolicy::Seen),
            "header_crc_ok" => Ok(NetIdCachingPolicy::HeaderCrcOk),
            "payload_crc_ok" => Ok(NetIdCachingPolicy::PayloadCrcOk),
            _ => Err(()),
        }
    }
}

struct State {
    m_state: DecoderState,       //< Current state of the synchronization
    m_center_freq: u32,          //< RF center frequency
    m_bw: Bandwidth,             //< Bandwidth
    m_sf: SpreadingFactor,       //< Spreading factor
    m_impl_head: bool,           //< use implicit header mode
    m_os_factor: usize,          //< oversampling factor
    m_n_up_req: SyncState,       //< number of consecutive upchirps required to trigger a detection
    m_number_of_bins: usize,     //< Number of bins in each lora Symbol
    m_samples_per_symbol: usize, //< Number of samples received per lora symbols
    m_symb_numb: usize,          //<number of payload lora symbols
    m_received_head: bool, //< indicate that the header has be decoded and received by this block
    snr_est: f64,          //< estimate of the snr
    in_down: Vec<Complex32>, //< downsampled input
    m_downchirp: Vec<Complex32>, //< Reference downchirp
    m_upchirp: Vec<Complex32>, //< Reference upchirp
    frame_cnt: usize,      //< Number of frame received
    symbol_cnt: SyncState, //< Number of symbols already received
    bin_idx: Option<usize>, //< value of previous lora symbol
    // bin_idx_new: i32, //< value of newly demodulated symbol
    m_preamb_len: usize,        //< Number of consecutive upchirps in preamble
    additional_upchirps: usize, //< indicate the number of additional upchirps found in preamble (in addition to the minimum required to trigger a detection)
    additional_symbol_samp: Vec<Complex32>, //< save the value of the last 1.25 downchirp as it might contain the first payload symbol
    preamble_raw: Vec<Complex32>, //<vector containing the preamble upchirps without any synchronization
    preamble_raw_up: Vec<Complex32>, //<vector containing the upsampled preamble upchirps without any synchronization
    preamble_upchirps: Vec<Complex32>, //<vector containing the preamble upchirps
    net_id_samp: Vec<Complex32>,     //< vector of the oversampled network identifier samples
    up_symb_to_use: usize, //< number of upchirp symbols to use for CFO and STO frac estimation
    k_hat: usize,          //< integer part of CFO+STO
    preamb_up_vals: Vec<usize>, //< value of the preamble upchirps
    m_cfo_frac: f64,       //< fractional part of CFO
    m_sto_frac: f32,       //< fractional part of CFO
    sfo_hat: f32,          //< estimated sampling frequency offset
    sfo_cum: f32,          //< cumulation of the sfo
    cfo_frac_sto_frac_est: bool, //< indicate that the estimation of CFO_frac and STO_frac has been performed
    cfo_frac_correc: Vec<Complex32>, //< cfo frac correction vector
    down_val: Option<usize>,     //< value of the preamble downchirps
    tag_from_msg_handler_to_work_channel: (mpsc::Sender<Pmt>, mpsc::Receiver<Pmt>),
    known_valid_net_ids: [[bool; 256]; 256],
    known_valid_net_ids_reverse: [[bool; 256]; 256],
    net_id: [u16; 2],
    ready_to_detect: bool,
    net_id_caching_policy: NetIdCachingPolicy,
    collect_receive_statistics: bool,
    receive_statistics_net_id_offset: i16,
    receive_statistics_one_symbol_off: bool,
    fft_forward_number_of_bins: Arc<dyn Fft<f32>>,
    fft_forward_two_times_number_of_bins: Arc<dyn Fft<f32>>,
    startup_timestamp_nanos: u64,
    processed_samples: u64,
}

impl State {
    fn estimate_cfo_frac_bernier(&self, samples: &[Complex32]) -> (Vec<Complex32>, f64) {
        let mut fft_val: Vec<Complex32> =
            vec![Complex32::new(0., 0.); self.up_symb_to_use * self.m_number_of_bins];
        let mut k0: Vec<usize> = vec![0; self.up_symb_to_use];
        let mut k0_mag: Vec<f64> = vec![0.; self.up_symb_to_use];
        for i in 0_usize..self.up_symb_to_use {
            // Dechirping
            let dechirped: Vec<Complex32> = volk_32fc_x2_multiply_32fc(
                &samples[(i * self.m_number_of_bins)..((i + 1) * self.m_number_of_bins)],
                &self.m_downchirp,
            );
            let mut cx_out_cfo: Vec<Complex32> = dechirped;
            // info!("dechirped: {}", cx_out_cfo.len());
            // do the FFT
            self.fft_forward_number_of_bins.process(&mut cx_out_cfo);
            let fft_mag_sq: Vec<f32> = volk_32fc_magnitude_squared_32f(&cx_out_cfo);
            fft_val[(i * self.m_number_of_bins)..((i + 1) * self.m_number_of_bins)]
                .copy_from_slice(&cx_out_cfo[0_usize..self.m_number_of_bins]);
            // Get magnitude
            // get argmax here
            k0[i] = argmax_f32(&fft_mag_sq);

            k0_mag[i] = fft_mag_sq[k0[i]] as f64;
        }
        // get argmax
        let idx_max: usize = k0[argmax_f64(&k0_mag)];
        let mut four_cum = Complex32::new(0., 0.);
        for i in 0_usize..(self.up_symb_to_use - 1) {
            four_cum += fft_val[idx_max + self.m_number_of_bins * i]
                * fft_val[idx_max + self.m_number_of_bins * (i + 1)].conj();
        }
        let cfo_frac = -four_cum.arg() as f64 / 2. / std::f64::consts::PI;
        // Correct CFO in preamble
        let cfo_frac_correc_aug: Vec<Complex32> = (0_usize
            ..(self.up_symb_to_use * self.m_number_of_bins))
            .map(|x| {
                Complex32::from_polar(
                    1.,
                    -2. * PI * cfo_frac as f32 / self.m_number_of_bins as f32 * x as f32,
                )
            })
            .collect();
        let preamble_upchirps = volk_32fc_x2_multiply_32fc(samples, &cfo_frac_correc_aug);
        (preamble_upchirps, cfo_frac)
    }

    /// estimates the fractional sampling time offset by computing spectral lines on the sum of the FFTs of the dechirped upchirps
    ///
    /// return value lies within ]-0.5, 0.5]
    fn estimate_sto_frac(&self) -> f32 {
        let mut fft_mag_sq: Vec<f32> = vec![0.; 2 * self.m_number_of_bins];
        for i in 0_usize..self.up_symb_to_use {
            // Dechirping
            let dechirped: Vec<Complex32> = volk_32fc_x2_multiply_32fc(
                &self.preamble_upchirps
                    [(self.m_number_of_bins * i)..(self.m_number_of_bins * (i + 1))],
                &self.m_downchirp,
            );

            let mut cx_out_sto: Vec<Complex32> =
                vec![Complex32::new(0., 0.); 2 * self.m_number_of_bins];
            cx_out_sto[..self.m_number_of_bins].copy_from_slice(&dechirped);
            // do the FFT
            self.fft_forward_two_times_number_of_bins
                .process(&mut cx_out_sto);
            // Get magnitude

            fft_mag_sq = volk_32fc_magnitude_squared_32f(&cx_out_sto)
                .iter()
                .zip(fft_mag_sq.iter())
                .map(|(x, y)| x + y)
                .collect();
        }

        // get argmax here
        let k0 = argmax_f32(&fft_mag_sq);

        // get three spectral lines
        let y_1 = fft_mag_sq[my_modulo(k0 as isize - 1, 2 * self.m_number_of_bins)] as f64;
        let y0 = fft_mag_sq[k0] as f64;
        let y1 = fft_mag_sq[(k0 + 1) % (2 * self.m_number_of_bins)] as f64;

        // set constant coeff
        let u = 64. * self.m_number_of_bins as f64 / 406.5506497; // from Cui yang (eq.15)
        let v = u * 2.4674;
        // RCTSL
        let wa = (y1 - y_1) / (u * (y1 + y_1) + v * y0);
        let ka = wa * self.m_number_of_bins as f64 / std::f64::consts::PI;
        // workaround to get modulo instead of remainder
        let k_residual = (((((k0 as f64 + ka) / 2.) % (1.)) + (1.)) % (1.)) as f32;
        // limit the value range and return
        k_residual - if k_residual > 0.5 { 1. } else { 0. }
    }

    fn determine_snr(&self, samples: &[Complex32]) -> f64 {
        // Multiply with ideal downchirp
        let mut dechirped = volk_32fc_x2_multiply_32fc(samples, &self.m_downchirp);
        // do the FFT
        self.fft_forward_number_of_bins.process(&mut dechirped);

        let fft_mag: Vec<f32> = dechirped.iter().map(|c| c.norm_sqr()).collect();
        let tot_en: f32 = fft_mag.iter().sum();
        if tot_en == 0. {
            return f64::NAN;
        }
        // Return argmax here
        let max_idx = argmax_f32(&fft_mag);
        let sig_en = fft_mag[max_idx];
        let noise_en = tot_en - sig_en;
        if noise_en == 0. {
            return f64::INFINITY;
        }
        10.0 * (sig_en as f64 / noise_en as f64).log10()
    }

    fn cache_current_net_id(&mut self) {
        if !self.known_valid_net_ids[self.net_id[0] as usize][self.net_id[1] as usize] {
            info!(
                "caching new net id: [{}, {}]",
                self.net_id[0], self.net_id[1]
            );
        }
        self.known_valid_net_ids[self.net_id[0] as usize][self.net_id[1] as usize] = true;
        self.known_valid_net_ids_reverse[self.net_id[1] as usize][self.net_id[0] as usize] = true;
    }

    fn detect(&mut self, input: &[Complex32]) -> (isize, usize) {
        let bin_idx_new_opt = get_symbol_val(
            &self.in_down,
            &self.m_downchirp,
            &self.fft_forward_number_of_bins,
        );

        if let Some(bin_idx_new) = bin_idx_new_opt {
            let bin_idx_matches_last_upchirp = match self.bin_idx {
                Some(last_bin_idx) => {
                    ((((bin_idx_new as i32 - last_bin_idx as i32).abs() + 1)
                        % self.m_number_of_bins as i32)
                        - 1)
                    .abs()
                        <= 1
                }
                None => false,
            };
            if bin_idx_matches_last_upchirp {
                self.preamb_up_vals[Into::<usize>::into(self.symbol_cnt)] = bin_idx_new;
                let preamble_raw_idx_offset =
                    self.m_number_of_bins * Into::<usize>::into(self.symbol_cnt);
                let count = self.m_number_of_bins;
                self.preamble_raw[preamble_raw_idx_offset..(preamble_raw_idx_offset + count)]
                    .copy_from_slice(&self.in_down[0..count]);
                let preamble_raw_up_idx_offset =
                    self.m_samples_per_symbol * Into::<usize>::into(self.symbol_cnt);
                let count = self.m_samples_per_symbol;
                self.preamble_raw_up
                    [preamble_raw_up_idx_offset..(preamble_raw_up_idx_offset + count)]
                    .copy_from_slice(
                        &input[(self.m_os_factor / 2)..(self.m_os_factor / 2 + count)],
                    );

                self.increase_symbol_count();
                // info!("symbol_cnt: {}", Into::<usize>::into(self.symbol_cnt));
            } else {
                self.preamb_up_vals[0] = bin_idx_new;
                let count = self.m_number_of_bins;
                self.preamble_raw[0..count].copy_from_slice(&self.in_down[0..count]);
                let count = self.m_samples_per_symbol;
                self.preamble_raw_up[0..count].copy_from_slice(
                    &input[(self.m_os_factor / 2)..(self.m_os_factor / 2 + count)],
                );

                self.transition_state(DecoderState::Detect, Some(SyncState::NetId2));
            }
            self.bin_idx = bin_idx_new_opt;
            let items_to_consume = if self.symbol_cnt == self.m_n_up_req {
                debug!(
                    "..:: Frame Detected ({:.1}MHz, SF{})",
                    self.m_center_freq as f32 / 1.0e6,
                    self.m_sf
                );
                // info!(
                //     "FrameSync: detected required nuber of upchirps ({})",
                //     Into::<usize>::into(self.m_n_up_req)
                // );
                self.additional_upchirps = 0;
                self.k_hat = most_frequent(&self.preamb_up_vals);
                let input_idx_offset = (0.75 * self.m_samples_per_symbol as f32
                    - self.k_hat as f32 * self.m_os_factor as f32)
                    as usize;
                let count = self.m_samples_per_symbol / 4;
                self.net_id_samp[0..count]
                    .copy_from_slice(&input[input_idx_offset..(input_idx_offset + count)]);

                // perform the coarse synchronization
                self.transition_state(DecoderState::Sync, Some(SyncState::NetId1));
                self.m_os_factor as isize * (self.m_number_of_bins as isize - self.k_hat as isize)
            } else {
                // info!(
                //     "FrameSync: did not detect required nuber of upchirps ({}/{})",
                //     Into::<usize>::into(self.symbol_cnt),
                //     Into::<usize>::into(self.m_n_up_req)
                // );
                self.m_samples_per_symbol as isize
            };
            (items_to_consume, 0)
        } else {
            // symbol had zero energy, thus can't be the beginning of the preamble -> reset to
            self.bin_idx = None;
            self.transition_state(DecoderState::Detect, Some(SyncState::NetId1));
            (self.m_samples_per_symbol as isize, 0)
        }
    }

    fn compute_sto_index(&self) -> usize {
        (Self::my_roundf(self.m_os_factor as f32 * (0.5 - self.m_sto_frac)) as usize)
            .min(self.m_os_factor - 1)
    }

    fn sync_quarter_down(
        &mut self,
        input: &[Complex32],
        out: &mut [Complex32],
        tags: &mut Tags,
        nitems_to_process: usize,
    ) -> (isize, usize) {
        let mut items_to_consume = ADDITIONAL_SAMPLES_FOR_NET_ID_RESYNCHING as isize; // always consume the remaining four samples of the second NetId (except when the offsets added below become more negative than 1/4 symbol length), left over by the previous state as a buffer
        let mut items_to_output = 0;
        let offset = items_to_consume as usize; // currently assumed start of the quarter down symbol, which might in fact already be payload (first m_samples_per_symbol already filled in sync->Down2)
        let count = self.m_samples_per_symbol;
        // requires self.m_samples_per_symbol samples in input buffer, but QuarterDown also needs 'backward' access to up to 3 samples for net-id re-synching when CFO is at minimum
        self.additional_symbol_samp[self.m_samples_per_symbol..(self.m_samples_per_symbol + count)]
            .copy_from_slice(&input[offset..(offset + count)]);
        let m_cfo_int = if let Some(down_val) = self.down_val {
            // info!("down_val: {}", down_val);
            // tuning CFO entails re-tuning STO (to not lose alignment of preamble upchirps) -> slope is normalized to 1 -> move half distance in frequency -> entails moving half distance in time -> aligns downchirp with sampling window, while keeping alignment of upchirps
            // if CFO_int is 0, this check is perfectly aligned with the second downchirp.
            // if it is less than 0, we have the full first downchirp before to still give a valid (modulated) downchirp
            // if it is higher than one, we have a quarter upchirp ahead, which is enough, as we can only (or rather assume to) be misaligned by not more than 1/4 symbol in time at this point. (re-aligning by 1/4 t_sym in time entails shifting by 1/4 BW in freqeuncy, which shifts symbols by half in the observed window due to aliasing)
            // otherwise we were off by more than 1/4 the BW in either direction, which is too much for this algorithm to compensate
            if down_val < self.m_number_of_bins / 2 {
                down_val as isize / 2
            } else {
                // if ahead by more than half a symbol, we are _probably_ actually behind by less than half a symbol
                (down_val as isize - self.m_number_of_bins as isize) / 2
            }
        } else {
            warn!("self.down_val must not be None here.");
            self.reset(false);
            return (self.m_samples_per_symbol as isize, 0);
        };

        let cfo_int_modulo = my_modulo(m_cfo_int, self.m_number_of_bins);

        // *******
        // re-estimate sto_frac and estimate SFO
        // *******
        // correct STOint and CFOint in the preamble upchirps
        // correct STOint
        self.preamble_upchirps.rotate_left(cfo_int_modulo);
        // correct CFOint
        let cfo_int_correc: Vec<Complex32> = (0_usize
            ..((Into::<usize>::into(self.m_n_up_req) + self.additional_upchirps)
                * self.m_number_of_bins))
            .map(|x| {
                Complex32::from_polar(
                    1.,
                    -2. * PI * m_cfo_int as f32 / self.m_number_of_bins as f32 * x as f32,
                )
            })
            .collect();
        self.preamble_upchirps =
            volk_32fc_x2_multiply_32fc(&self.preamble_upchirps, &cfo_int_correc); // count: up_symb_to_use * m_number_of_bins
        // correct SFO in the preamble upchirps
        // SFO times symbol duration = number of samples we need to compensate the symbol duration by
        // small, as m_cfo_int+self.m_cfo_frac bounded by ]-(self.m_number_of_bins/4+0.5),+(self.m_number_of_bins/4+0.5)[
        // BW/s_f = 1/6400 @ 125kHz, 800mHz
        // -> sfo_hat ~ ]-1/250,+1/250[
        self.sfo_hat = (m_cfo_int as f32 + self.m_cfo_frac as f32) * Into::<f32>::into(self.m_bw)
            / self.m_center_freq as f32;
        // CFO normalized to carrier frequency / SFO times t_samp
        let clk_off = self.sfo_hat as f64 / self.m_number_of_bins as f64;
        let fs: f64 = self.m_bw.into();
        // we wanted f_c_true, got f_c_true+cfo_int+cfo_fraq -> f_c = f_c_true-cfo_int-cfo_fraq -> f_c = f_c_true - (cfo_int+cfo_fraq) -> normalized "clock offset" of f_c/f_c_true=1-(cfo_int+cfo_fraq)/f_c_true
        // assume linear offset: we wanted BW, assume we got fs_p=BW-(cfo_int+cfo_fraq)/f_c_true*BW -> fs_p=BW*(1-(cfo_int+cfo_fraq)/f_c_true)
        // compute actual sampling frequency fs_p to be able to compensate
        let fs_p = fs * (1. - clk_off);
        let n = self.m_number_of_bins;
        // correct SFO
        let sfo_corr_vect: Vec<Complex32> = (0..((Into::<usize>::into(self.m_n_up_req)
            + self.additional_upchirps)
            * self.m_number_of_bins))
            .map(|x| {
                Complex32::from_polar(
                    1.,
                    (-2. * std::f64::consts::PI * ((x % n) * (x % n)) as f64 / 2. / n as f64
                        * (fs / fs_p * fs / fs_p - 1.)
                        + ((x / n) as f64 * (fs / fs_p * fs / fs_p - fs / fs_p)
                            + fs / 2. * (1. / fs - 1. / fs_p))
                            * (x % n) as f64) as f32,
                )
            })
            .collect();
        let count = self.up_symb_to_use * self.m_number_of_bins;
        let tmp =
            volk_32fc_x2_multiply_32fc(&self.preamble_upchirps[0..count], &sfo_corr_vect[0..count]);
        self.preamble_upchirps[0..count].copy_from_slice(&tmp);
        // re-estimate sto_frac based on the now corrected self.preamble_upchirps to get better estimate than in the beginning of the upchirps
        let tmp_sto_frac = self.estimate_sto_frac();
        let diff_sto_frac = self.m_sto_frac - tmp_sto_frac; // both bounded by ]-0.5, 0.5] -> diff bounded by ]-1.0, 1.0[
        if diff_sto_frac.abs() <= (self.m_os_factor - 1) as f32 / self.m_os_factor as f32 {
            // avoid introducing off-by-one errors by estimating fine_sto=-0.499 , rough_sto=0.499
            self.m_sto_frac = tmp_sto_frac;
        }

        // get SNR estimate from preamble
        // downsample preab_raw
        // apply sto correction
        let preamble_raw_up_offset =
            ((self.m_os_factor * (self.m_number_of_bins - self.k_hat)) as isize
                - Self::my_roundf(self.m_os_factor as f32 * self.m_sto_frac)) as usize;
        let count = (Into::<usize>::into(self.m_n_up_req) + self.additional_upchirps)
            * self.m_number_of_bins;
        let mut corr_preamb: Vec<Complex32> = self.preamble_raw_up
            [preamble_raw_up_offset..(preamble_raw_up_offset + self.m_os_factor * count)]
            .iter()
            .step_by(self.m_os_factor)
            .copied()
            .collect();
        corr_preamb.rotate_left(cfo_int_modulo);
        // apply cfo correction
        corr_preamb = volk_32fc_x2_multiply_32fc(&corr_preamb, &cfo_int_correc);
        for i in 0..(Into::<usize>::into(self.m_n_up_req) + self.additional_upchirps) {
            let offset = self.m_number_of_bins * i;
            let end_range = self.m_number_of_bins * (i + 1);
            let tmp = volk_32fc_x2_multiply_32fc(
                &corr_preamb[offset..end_range],
                &self.cfo_frac_correc[0..self.m_number_of_bins],
            );
            corr_preamb[offset..end_range].copy_from_slice(&tmp);
        }

        // apply sfo correction
        corr_preamb = volk_32fc_x2_multiply_32fc(&corr_preamb, &sfo_corr_vect);

        self.snr_est = 0.0_f64;
        for i in 0..self.up_symb_to_use {
            self.snr_est += self.determine_snr(
                &corr_preamb[(i * self.m_number_of_bins)..((i + 1) * self.m_number_of_bins)],
            );
        }
        self.snr_est /= self.up_symb_to_use as f64;

        // update sto_frac to its value at the beginning of the net id
        self.m_sto_frac += self.sfo_hat * self.m_preamb_len as f32;
        // ensure that m_sto_frac is in ]-0.5,0.5]
        // STO_int is already at state of NetID 1 (due to re-synching via net_id), ignore additional int offset normally created by wrapping around the fractional offset
        if self.m_sto_frac > 0.5 {
            self.m_sto_frac -= 1.0;
            items_to_consume += 1;
        } else if self.m_sto_frac <= -0.5 {
            self.m_sto_frac += 1.0;
            items_to_consume -= 1;
        }
        // decim net id according to new sto_frac and sto int
        // start_off gives the offset in the net_id_samp vector required to be aligned in time (CFOint is equivalent to STOint at this point, since upchirp_val was forced to 0, and initial alignment has already been performed. note that CFOint here is only the remainder of STOint that needs to be re-aligned.)
        let start_off = (self.compute_sto_index() as isize
            // self.m_number_of_bins as isize / 4 is manually introduced static NET_ID_1 start offset in array before CFO alignment to be able to align negative m_cfo_int offsets
            + self.m_os_factor as isize * (self.m_number_of_bins as isize / 4 + m_cfo_int))
            as usize;
        let count = 2 * self.m_number_of_bins;
        let mut net_ids_samp_dec: Vec<Complex32> = self.net_id_samp
            [start_off..(start_off + count * self.m_os_factor)]
            .iter()
            .step_by(self.m_os_factor)
            .copied()
            .collect();
        net_ids_samp_dec = volk_32fc_x2_multiply_32fc(&net_ids_samp_dec, &cfo_int_correc);

        // correct CFO_frac in the network ids
        let tmp = volk_32fc_x2_multiply_32fc(
            &net_ids_samp_dec[0..self.m_number_of_bins],
            &self.cfo_frac_correc,
        );
        net_ids_samp_dec[0..self.m_number_of_bins].copy_from_slice(&tmp);
        let tmp = volk_32fc_x2_multiply_32fc(
            &net_ids_samp_dec[self.m_number_of_bins..(2 * self.m_number_of_bins)],
            &self.cfo_frac_correc,
        );
        net_ids_samp_dec[self.m_number_of_bins..(2 * self.m_number_of_bins)].copy_from_slice(&tmp);

        let net_id_0_tmp = get_symbol_val(
            &net_ids_samp_dec[0..self.m_number_of_bins],
            &self.m_downchirp,
            &self.fft_forward_number_of_bins,
        );
        if net_id_0_tmp.is_none() {
            warn!("FrameSync: encountered symbol with signal energy 0.0, aborting.");
            self.reset(true);
            return (items_to_consume, 0);
        }
        self.net_id[0] = net_id_0_tmp
            .unwrap()
            .try_into()
            .expect("net-id can't be greater than SF bits, with SF<=12.");
        let net_id_1_tmp = get_symbol_val(
            &net_ids_samp_dec[self.m_number_of_bins..(2 * self.m_number_of_bins)],
            &self.m_downchirp,
            &self.fft_forward_number_of_bins,
        );
        if net_id_1_tmp.is_none() {
            warn!("FrameSync: encountered symbol with signal energy 0.0, aborting.");
            self.reset(true);
            return (items_to_consume, 0);
        }
        self.net_id[1] = net_id_1_tmp
            .unwrap()
            .try_into()
            .expect("net-id can't be greater than SF bits, with SF<=12.");
        let mut one_symbol_off = false;

        // info!("netid1: {} (soll {})", self.net_id[0], self.m_sync_words[0]);
        // info!("netid2: {} (soll {})", self.net_id[1], self.m_sync_words[1]);
        // info!("raw netid1: {}", self.net_id[0]);
        // info!("raw netid2: {}", self.net_id[1]);
        // the last three bits of netID o and 1 are always 0 -> margin of 3 in either direction to refine CFO_int
        // TODO with the SX1302 at least, the 0x04 bit can also be set, leaving only two always-zero bits. In practice, however, this is not used and also not exposed by the HAL, so can still be assumed to be zero.
        let net_id_off_raw = self.net_id[0] & 0x07;
        let net_id_off_raw_1 = self.net_id[1] & 0x07;
        if net_id_off_raw == 0x04 {
            debug!("FrameSync: bad sync: offset > 3");
            self.reset(true);
            return (0, 0);
        }
        // we have detected a syntactically valid net ID,
        let net_id_off: i16 = if net_id_off_raw > 4 {
            // closer to the next higher net_id
            self.net_id[0] += 0x08;
            // frequencies outside the BW alias and wrap around, preserving the chirp structure,
            // but only if we oversample at the SDR to include the guard bands and then subsample
            // without a lowpass filter, i.e. when self.os_factor > 1
            self.net_id[0] %= self.m_number_of_bins as u16;
            // assume same offset for both parts, verify below
            self.net_id[1] += 0x08;
            self.net_id[1] %= self.m_number_of_bins as u16;
            net_id_off_raw as i16 - 0x08
        } else {
            net_id_off_raw as i16
        };
        if net_id_off != 0 && net_id_off.abs() > 1 {
            debug!("[frame_sync.rs] net id offset >1: {}", net_id_off);
        }
        // discard the lower bits introduced by the offset
        self.net_id[0] &= 0xF8;
        self.net_id[1] &= 0xF8;
        if net_id_off.unsigned_abs() as usize > MAX_UNKNOWN_NET_ID_OFFSET
            && !self.known_valid_net_ids[self.net_id[0] as usize][self.net_id[1] as usize]
        {
            debug!(
                "FrameSync: bad sync: previously unknown NetID and offset > {MAX_UNKNOWN_NET_ID_OFFSET}"
            );
            self.reset(true);
            return (0, 0);
        }

        if net_id_off_raw != net_id_off_raw_1 {
            // check if we are in fact checking the second net ID and that the first one was considered as a preamble upchirp
            self.net_id[1] = self.net_id[0];
            let i = Into::<usize>::into(self.m_n_up_req) + self.additional_upchirps - 1;
            let net_id_1_tmp = get_symbol_val(
                &corr_preamb[(i * self.m_number_of_bins)..((i + 1) * self.m_number_of_bins)],
                &self.m_downchirp,
                &self.fft_forward_number_of_bins,
            );
            if net_id_1_tmp.is_none() {
                warn!("encountered symbol with signal energy 0.0, aborting.");
                self.reset(true);
                return (items_to_consume, 0);
            }
            let net_id_1_tmp: u16 = net_id_1_tmp
                .unwrap()
                .try_into()
                .expect("net-id can't be greater than SF bits, with SF<=12.");
            if net_id_1_tmp & 0x07 != net_id_off_raw {
                debug!(
                    "FrameSync: bad sync: different offset for recovered net_id[0] and net_id[1]"
                );
                self.reset(true);
                return (items_to_consume, 0);
            } else if !self.known_valid_net_ids_reverse[self.net_id[0] as usize]
                [(net_id_1_tmp & 0xF8) as usize]
            {
                debug!(
                    "FrameSync: bad sync: different offset for original net_id[0] and net_id[1] and no match for recovered net_id[0]"
                );
                self.reset(true);
                return (items_to_consume, 0);
            } else {
                debug!("detected netid2 as netid1, recovering..");
                self.net_id[0] = net_id_1_tmp;
                // info!("netid1: {}", self.net_id[0]);
                // info!("netid2: {}", self.net_id[1]);
                one_symbol_off = true;
                items_to_consume -= -(self.m_os_factor as isize) * net_id_off as isize;
                // the first symbol was mistaken for the end of the downchirp. we should correct and output it.
                let start_off = self.m_os_factor as isize / 2  // start half a sample delayed to have a buffer for the following STOfrac (of value +-1/2 sample) ->
                    - Self::my_roundf(self.m_sto_frac * self.m_os_factor as f32)
                    + self.m_os_factor as isize * (self.m_number_of_bins as isize / 4 + m_cfo_int);
                for i in (start_off..(self.m_samples_per_symbol as isize * 5 / 4))
                    .step_by(self.m_os_factor)
                {
                    // assert!((i - start_off) >= 0);
                    // assert!(i >= 0);  // first term of start_off >= 0, second term >= 0 (m_cfo_int >= -m_number_of_bins/4)
                    out[(i - start_off) as usize / self.m_os_factor] =
                        self.additional_symbol_samp[i as usize];
                }
                items_to_output = self.m_number_of_bins;
                self.frame_cnt += 1;
            }
        } else {
            info!("detected syntactically correct net_id with matching offset {net_id_off}");
            if self.net_id_caching_policy == NetIdCachingPolicy::Seen {
                self.cache_current_net_id();
            }
            // info!("netid1: {}", self.net_id[0]);
            // info!("netid2: {}", self.net_id[1]);
            // correct remaining offset in time only, as correction in frequency only necessary for estimating SFO (by first estimating CFO), and SFO is only estimated once per frame.
            // can shift by additional 3 samples -> "buffer" (not zet consumed samples up to the beginning of the payload) needs to be |min(m_cfo_int)|+3 = m_samples_per_symbol/4+3 samples long
            items_to_consume += -(self.m_os_factor as isize) * net_id_off as isize;
            self.frame_cnt += 1;
            if !self.known_valid_net_ids[self.net_id[0] as usize][self.net_id[1] as usize] {
                info!(
                    "encountered new net id: [{}, {}], trying to decode header...",
                    self.net_id[0], self.net_id[1]
                );
            }
        }
        info!("SNR: {}dB", self.snr_est);
        info!("Net-ID: [{}, {}]", self.net_id[0], self.net_id[1]);
        // net IDs syntactically correct and matching offset => frame detected, proceed with trying to decode the header
        self.m_received_head = false;
        // consume the quarter downchirp, and at the same time correct CFOint (already applied correction for NET_ID recovery was only in retrospect on a local buffer)
        items_to_consume += self.m_samples_per_symbol as isize / 4
            + self.m_os_factor as isize * m_cfo_int
            - self.m_os_factor as isize // quarter donwchirp is actually one baseband-sample shorter, and subsequent symbols are shifted. Necessary for reliable SF5 and SF6 transmission to commercial devices (tested against SX1302).
        ;
        assert!(
            items_to_consume <= nitems_to_process as isize,
            "must not happen, we already altered persistent state."
        );
        // info!("Frame Detected.");
        // update sto_frac to its value at the payload beginning
        // self.m_sto_frac can now be slightly outside ]-0.5,0.5] samples
        self.m_sto_frac += self.sfo_hat * 4.25;
        // STO_int (=k_hat - CFO_int) was last updated at start of netID -> shift sto_frac back into ]-0.5,0.5] range and add possible offset of one to STO_int (by consuming one sample more or less)
        if self.m_sto_frac > 0.5 {
            self.m_sto_frac -= 1.0;
            items_to_consume += 1;
        } else if self.m_sto_frac <= -0.5 {
            self.m_sto_frac += 1.0;
            items_to_consume -= 1;
        }
        self.sfo_cum = ((self.m_sto_frac * self.m_os_factor as f32)
            - Self::my_roundf(self.m_sto_frac * self.m_os_factor as f32) as f32)
            / self.m_os_factor as f32;

        if self.m_sf < SpreadingFactor::SF7
        // Semtech adds two null symbol in the beginning. Maybe for additional synchronization?
        {
            items_to_consume += 2 * self.m_samples_per_symbol as isize;
        }

        let mut frame_info: HashMap<String, Pmt> = HashMap::new();

        frame_info.insert(String::from("is_header"), Pmt::Bool(true));
        frame_info.insert(String::from("cfo_int"), Pmt::Isize(m_cfo_int));
        frame_info.insert(String::from("cfo_frac"), Pmt::F64(self.m_cfo_frac));
        frame_info.insert(String::from("sf"), Pmt::Usize(self.m_sf.into()));
        frame_info.insert(
            String::from("timestamp"),
            Pmt::U64(
                self.startup_timestamp_nanos
                    + (self.processed_samples
                        + items_to_consume as u64
                        + if one_symbol_off {
                            self.m_samples_per_symbol as u64
                        } else {
                            0
                        })
                        * 1_000_000_000
                        / ((Into::<usize>::into(self.m_bw) * self.m_os_factor) as u64),
            ),
        );
        let frame_info_pmt = Pmt::MapStrPmt(frame_info);

        debug!(
            "detected frame ({}MHz, SF {})",
            self.m_center_freq, self.m_sf
        );

        tags.add_tag(
            0,
            Tag::NamedAny("frame_info".to_string(), Box::new(frame_info_pmt)),
        );
        if self.collect_receive_statistics {
            self.receive_statistics_net_id_offset = net_id_off;
            self.receive_statistics_one_symbol_off = one_symbol_off;
        }
        if one_symbol_off {
            self.transition_state(DecoderState::SfoCompensation, Some(SyncState::NetId2));
        } else {
            self.transition_state(DecoderState::SfoCompensation, Some(SyncState::NetId1));
        };

        (items_to_consume, items_to_output)
    }

    fn sync(
        &mut self,
        input: &[Complex32],
        out: &mut [Complex32],
        tags: &mut Tags,
        nitems_to_process: usize,
    ) -> (isize, usize) {
        let mut items_to_output = 0;
        if !self.cfo_frac_sto_frac_est {
            let (preamble_upchirps_tmp, cfo_frac_tmp) = self.estimate_cfo_frac_bernier(
                &self.preamble_raw[(self.m_number_of_bins - self.k_hat)..],
            );
            self.preamble_upchirps = preamble_upchirps_tmp;
            self.m_cfo_frac = cfo_frac_tmp;
            self.m_sto_frac = self.estimate_sto_frac();
            // create correction vector
            self.cfo_frac_correc = (0..self.m_number_of_bins)
                .map(|x| {
                    Complex32::from_polar(
                        1.,
                        -2. * PI * self.m_cfo_frac as f32 / self.m_number_of_bins as f32 * x as f32,
                    )
                })
                .collect();
            self.cfo_frac_sto_frac_est = true;
        }
        let mut items_to_consume = self.m_samples_per_symbol as isize;
        // apply cfo correction
        let symb_corr = volk_32fc_x2_multiply_32fc(&self.in_down, &self.cfo_frac_correc);

        self.bin_idx = get_symbol_val(
            &symb_corr,
            &self.m_downchirp,
            &self.fft_forward_number_of_bins,
        );
        match self.symbol_cnt {
            SyncState::NetId1 => {
                if self.bin_idx.is_some()
                    && (self.bin_idx.unwrap() == 0
                        || self.bin_idx.unwrap() == 1
                        || self.bin_idx.unwrap() == self.m_number_of_bins - 1)
                {
                    // look for additional upchirps. Won't work if network identifier 1 equals 2^sf-1, 0 or 1!
                    let input_offset = (0.75 * self.m_samples_per_symbol as f32) as usize;
                    let count = self.m_samples_per_symbol / 4;
                    self.net_id_samp[0..count]
                        .copy_from_slice(&input[input_offset..(input_offset + count)]);
                    if self.additional_upchirps >= 3 {
                        self.preamble_raw_up.rotate_left(self.m_samples_per_symbol);
                        let preamble_raw_up_offset =
                            self.m_samples_per_symbol * (Into::<usize>::into(self.m_n_up_req) + 3);
                        let input_offset = self.m_os_factor / 2 + self.k_hat * self.m_os_factor;
                        let count = self.m_samples_per_symbol;
                        self.preamble_raw_up
                            [preamble_raw_up_offset..(preamble_raw_up_offset + count)]
                            .copy_from_slice(&input[input_offset..(input_offset + count)]);
                    } else {
                        let preamble_raw_up_offset = self.m_samples_per_symbol
                            * (Into::<usize>::into(self.m_n_up_req) + self.additional_upchirps);
                        let input_offset = self.m_os_factor / 2 + self.k_hat * self.m_os_factor;
                        let count = self.m_samples_per_symbol;
                        self.preamble_raw_up
                            [preamble_raw_up_offset..(preamble_raw_up_offset + count)]
                            .copy_from_slice(&input[input_offset..(input_offset + count)]);
                        self.additional_upchirps += 1;
                    }
                } else {
                    // network identifier 1 correct or off by one
                    let net_id_samp_offset = self.m_samples_per_symbol / 4;
                    let count = self.m_samples_per_symbol;
                    self.net_id_samp[net_id_samp_offset..(net_id_samp_offset + count)]
                        .copy_from_slice(&input[0..count]);
                    self.transition_state(DecoderState::Sync, Some(SyncState::NetId2));
                }
            }
            SyncState::NetId2 => {
                let net_id_samp_offset = self.m_samples_per_symbol * 5 / 4;
                let count = (self.m_number_of_bins + 1) * self.m_os_factor;
                self.net_id_samp[net_id_samp_offset..(net_id_samp_offset + count)]
                    .copy_from_slice(&input[0..count]);
                self.transition_state(DecoderState::Sync, Some(SyncState::Downchirp1));
            }
            SyncState::Downchirp1 => {
                let net_id_samp_offset = self.m_samples_per_symbol * 9 / 4;
                let count = self.m_samples_per_symbol / 4;
                self.net_id_samp[net_id_samp_offset..(net_id_samp_offset + count)]
                    .copy_from_slice(&input[0..count]);
                self.transition_state(DecoderState::Sync, Some(SyncState::Downchirp2));
            }
            SyncState::Downchirp2 => {
                self.down_val = get_symbol_val(
                    &symb_corr,
                    &self.m_upchirp,
                    &self.fft_forward_number_of_bins,
                );
                // info!("self.down_val: {}", self.down_val.unwrap());
                let count = self.m_samples_per_symbol;
                self.additional_symbol_samp[0..count].copy_from_slice(&input[0..count]);
                self.transition_state(DecoderState::Sync, Some(SyncState::QuarterDown));
                // do not consume the last four samples of the symbol yet, as we need an additional buffer in the following QuarterDown state to cope with severely negative STO_int in combination with negative re-synching due to NetId offset and fractional STO <-0.5 due to SFO evolution
                // also work only ensures that there is one full symbol plus these four in the input queue before calling the subfunctions, so by leaving 4 samples of the second downchirp, we will have 1/4 symbol time + 4 samples to sync backward, but also a full new symbol for the additional_symbol_samp buffer
                (items_to_consume, items_to_output) = (
                    self.m_samples_per_symbol as isize
                        - ADDITIONAL_SAMPLES_FOR_NET_ID_RESYNCHING as isize,
                    0,
                );
            }
            SyncState::QuarterDown => {
                (items_to_consume, items_to_output) =
                    self.sync_quarter_down(input, out, tags, nitems_to_process);
            }
            _ => warn!("encountered unexpercted symbol_cnt SyncState."),
        }
        (items_to_consume, items_to_output)
    }

    fn compensate_sfo(
        &mut self,
        // input: &[Complex32],
        out: &mut [Complex32],
        tags: &mut Tags,
    ) -> (isize, usize) {
        if let Ok(Some(Pmt::MapStrPmt(mut frame_info))) =
            self.tag_from_msg_handler_to_work_channel.1.try_next()
        {
            // info!("new frame_info tag: {:?}", frame_info_tag);
            if self.collect_receive_statistics {
                frame_info.insert(
                    String::from("net_id_off"),
                    Pmt::Isize(self.receive_statistics_net_id_offset as isize),
                );
                frame_info.insert(
                    String::from("one_symbol_off"),
                    Pmt::Bool(self.receive_statistics_one_symbol_off),
                );
            }
            // frame_info.insert(String::from("cfo_int"), Pmt::Isize(self.m_cfo_int));
            // frame_info.insert(String::from("cfo_frac"), Pmt::F64(self.m_cfo_frac));
            frame_info.insert(String::from("snr"), Pmt::F64(self.snr_est));
            tags.add_tag(
                0,
                Tag::NamedAny(
                    "frame_info".to_string(),
                    Box::new(Pmt::MapStrPmt(frame_info)),
                ),
            );
        }

        // transmit only useful symbols (at least 8 symbol for PHY header)  // TODO implicit header?
        if Into::<usize>::into(self.symbol_cnt) < 8
            || (Into::<usize>::into(self.symbol_cnt) < self.m_symb_numb && self.m_received_head)
        {
            // info!("self.symbol_cnt: {}", Into::<usize>::into(self.symbol_cnt));
            // output downsampled signal (with no STO but with CFO)
            let count = self.m_number_of_bins;
            out[0..count].copy_from_slice(&self.in_down[0..count]);
            let mut items_to_consume = self.m_samples_per_symbol as isize;

            // update sfo evolution
            // if cumulative SFO is greater than 1/2 microsample duration
            debug_assert!(
                self.sfo_cum.abs() <= 1.5 / self.m_os_factor as f32,
                "cumulative SFO is greater than one microsample: {}",
                self.sfo_cum
            );
            if self.sfo_cum.abs() > 0.5 / self.m_os_factor as f32 {
                // sfo_cum starts out in range ]-0.5,0.5[ microsamples, sfo_hat is generally multiple orders of magnitude smaller than one microsample (at reasonable os_factors) -> sfo_cum can surpass the threshold, but not "overshoot" by more than one microsample -> it is sufficient to compensate by one microsample
                items_to_consume -= self.sfo_cum.signum() as isize;
                self.sfo_cum -= self.sfo_cum.signum() / self.m_os_factor as f32;
            }
            self.sfo_cum += self.sfo_hat;
            self.increase_symbol_count();
            (items_to_consume, self.m_number_of_bins)
        } else if !self.m_received_head {
            // Wait for the header to be decoded
            // called again when header decoded message ('frame_info') arrives
            (0, 0)
        } else {
            // frame fully received
            self.reset(false);
            (self.m_samples_per_symbol as isize, 0)
        }
    }

    /// stay in the same outer state and inrease the symbol count by one
    fn increase_symbol_count(&mut self) {
        self.symbol_cnt = From::<usize>::from(Into::<usize>::into(self.symbol_cnt) + 1_usize);
    }

    /// enter a new outer state and optionally change the symbol count
    fn transition_state(
        &mut self,
        new_decoder_state: DecoderState,
        new_sync_state: Option<SyncState>,
    ) {
        self.m_state = new_decoder_state;
        if let Some(sync_state) = new_sync_state {
            self.symbol_cnt = sync_state;
        }
    }

    fn next_iteration_possible(
        &self,
        samples_left_in_input: usize,
        space_left_in_output: usize,
    ) -> bool {
        if samples_left_in_input < self.m_samples_per_symbol {
            // required for downsampling the input, independent of state
            return false;
        }
        match self.m_state {
            DecoderState::Detect => {
                (self.net_id_caching_policy != NetIdCachingPolicy::PayloadCrcOk
                    || self.ready_to_detect)
                    && samples_left_in_input >= self.m_os_factor / 2 + self.m_samples_per_symbol
            }
            DecoderState::Sync => match self.symbol_cnt {
                SyncState::NetId1 => {
                    samples_left_in_input
                        >= self.m_os_factor / 2
                            + self.k_hat * self.m_os_factor
                            + self.m_samples_per_symbol
                }
                SyncState::NetId2 => {
                    samples_left_in_input > (self.m_number_of_bins + 1) * self.m_os_factor
                }
                SyncState::Downchirp1 => samples_left_in_input >= self.m_samples_per_symbol, // (already ensured above)
                SyncState::Downchirp2 => {
                    samples_left_in_input
                        >= self.m_samples_per_symbol - ADDITIONAL_SAMPLES_FOR_NET_ID_RESYNCHING
                }
                SyncState::QuarterDown => {
                    samples_left_in_input
                        >= (self.m_samples_per_symbol / 4
                            + self.m_os_factor
                                * (self.m_number_of_bins / 2
                                    + ADDITIONAL_SAMPLES_FOR_NET_ID_RESYNCHING)
                            + 2)
                        .max(self.m_samples_per_symbol + ADDITIONAL_SAMPLES_FOR_NET_ID_RESYNCHING)
                            + if self.m_sf < SpreadingFactor::SF7 {
                                2 * self.m_samples_per_symbol
                            } else {
                                0
                            }
                        && space_left_in_output >= self.m_number_of_bins
                }
                _ => panic!("encountered unexpercted symbol_cnt SyncState."),
            },
            DecoderState::SfoCompensation => {
                if Into::<usize>::into(self.symbol_cnt) >= 8 && !self.m_received_head {
                    return false;
                }
                // input needs +1 (micro)sample for cumulative SFO correction
                samples_left_in_input > self.m_samples_per_symbol
                    && space_left_in_output >= self.m_number_of_bins
            }
        }
    }

    /// reset STO and CFO estimates and reset state to Detect the next frame
    fn reset(&mut self, keep_last_symbol: bool) {
        if !keep_last_symbol {
            self.bin_idx = None;
            self.transition_state(DecoderState::Detect, Some(SyncState::NetId1));
        } else {
            if let Some(val) = self.bin_idx {
                self.preamb_up_vals[0] = val;
            }
            self.transition_state(DecoderState::Detect, Some(SyncState::NetId2));
        }
        self.k_hat = 0;
        self.m_cfo_frac = 0.;
        self.m_sto_frac = 0.;
        self.sfo_hat = 0.;
        self.sfo_cum = 0.;
        let _ = self.tag_from_msg_handler_to_work_channel.1.try_next();
        self.cfo_frac_sto_frac_est = false;
        self.ready_to_detect = true;
    }

    fn my_roundf(number: f32) -> isize {
        if number > 0.0 {
            (number + 0.5) as isize
        } else {
            (number - 0.5).ceil() as isize
        }
    }
}

const ADDITIONAL_SAMPLES_FOR_NET_ID_RESYNCHING: usize = 4; // might need to consider os_factor
const MAX_UNKNOWN_NET_ID_OFFSET: usize = 1;

#[derive(Block)]
#[message_inputs(bandwidth, center_freq, frame_info, payload_crc_result, poke)]
#[message_outputs(net_id_caching, frame_detected, detection_failed)]
pub struct FrameSync<I = DefaultCpuReader<Complex32>, O = DefaultCpuWriter<Complex32>>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    #[input]
    input: I,
    #[output]
    output: O,
    s: State,
}

impl<I, O> FrameSync<I, O>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        channel: Channel,
        bandwidth: Bandwidth,
        sf: SpreadingFactor,
        impl_head: bool,
        initial_sync_words: Vec<Vec<usize>>, // initially known NetIDs
        os_factor: usize,
        preamble_len: Option<usize>,
        net_id_caching_policy: Option<&str>,
        collect_receive_statistics: bool,
        startup_timestamp: Option<SystemTime>,
    ) -> Self {
        let net_id_caching_policy_tmp = match NetIdCachingPolicy::from_str(
            net_id_caching_policy.unwrap_or("header_crc_ok"),
        ) {
            Ok(tmp) => tmp,
            Err(_) => panic!(
                "Supplied invalid value for parameter net_id_caching_policy. Possible values: 'none', 'seen', 'header_crc_ok', 'payload_crc_ok'"
            ),
        };
        let preamble_len_tmp = preamble_len.unwrap_or(8);
        if preamble_len_tmp < 5 {
            panic!("Preamble length should be greater than 5!"); // only warning in original implementation
        }
        // NetID caching structure
        let mut known_valid_net_ids: [[bool; 256]; 256] = [[false; 256]; 256];
        let mut known_valid_net_ids_reverse: [[bool; 256]; 256] = [[false; 256]; 256];
        for sync_word in initial_sync_words {
            let sync_word_tmp: Vec<usize> = expand_sync_word(sync_word);
            if sync_word_tmp.len() == 2 {
                known_valid_net_ids_reverse[sync_word_tmp[1]][sync_word_tmp[0]] = true;
                known_valid_net_ids[sync_word_tmp[0]][sync_word_tmp[1]] = true;
            }
        }
        let m_number_of_bins_tmp = sf.samples_per_symbol();
        let m_samples_per_symbol_tmp = m_number_of_bins_tmp * os_factor;
        let (m_upchirp_tmp, m_downchirp_tmp) = build_ref_chirps(sf, 1);

        let fft_detect = FftPlanner::new().plan_fft(m_number_of_bins_tmp, FftDirection::Forward);

        let mut input = I::default();
        input.set_min_items(m_samples_per_symbol_tmp * 2 + os_factor / 2);

        let mut output = O::default();
        output.set_min_items(m_number_of_bins_tmp);

        Self {
            input,
            output,
            s: State {
                m_state: DecoderState::Detect,  //< Current state of the synchronization
                m_center_freq: channel.into(),  //< RF center frequency
                m_bw: bandwidth,                //< Bandwidth
                m_sf: sf,                       //< Spreading factor
                m_os_factor: os_factor,         //< oversampling factor
                m_preamb_len: preamble_len_tmp, //< Number of consecutive upchirps in preamble
                m_n_up_req: From::<usize>::from(preamble_len_tmp - 3), //< number of consecutive upchirps required to trigger a detection
                up_symb_to_use: preamble_len_tmp - 4, //< number of upchirp symbols to use for CFO and STO frac estimation
                m_sto_frac: 0.0,                      //< fractional part of CFO
                m_impl_head: impl_head,               //< use implicit header mode
                m_number_of_bins: m_number_of_bins_tmp, //< Number of bins in each lora Symbol
                m_samples_per_symbol: m_samples_per_symbol_tmp, //< Number of samples received per lora symbols
                additional_symbol_samp: vec![Complex32::new(0., 0.); 2 * m_samples_per_symbol_tmp], //< save the value of the last 1.25 downchirp as it might contain the first payload symbol
                m_upchirp: m_upchirp_tmp,     //< Reference upchirp
                m_downchirp: m_downchirp_tmp, //< Reference downchirp
                preamble_upchirps: vec![
                    Complex32::new(0., 0.);
                    preamble_len_tmp * m_number_of_bins_tmp
                ], //<vector containing the preamble upchirps
                preamble_raw_up: vec![
                    Complex32::new(0., 0.);
                    (preamble_len_tmp + 3) * m_samples_per_symbol_tmp
                ], //<vector containing the upsampled preamble upchirps without any synchronization
                cfo_frac_correc: vec![Complex32::new(0., 0.); m_number_of_bins_tmp], //< cfo frac correction vector
                // cfo_sfo_frac_correc: vec![Complex32::new(0., 0.); m_number_of_bins_tmp], //< correction vector accounting for cfo and sfo
                // symb_corr: vec![Complex32::new(0., 0.); m_number_of_bins_tmp], //< symbol with CFO frac corrected
                in_down: vec![Complex32::new(0., 0.); m_number_of_bins_tmp], //< downsampled input
                preamble_raw: vec![Complex32::new(0., 0.); m_number_of_bins_tmp * preamble_len_tmp], //<vector containing the preamble upchirps without any synchronization
                net_id_samp: vec![
                    Complex32::new(0., 0.);
                    (m_samples_per_symbol_tmp as f32 * 2.5) as usize
                ], //< vector of the oversampled network identifier samples
                bin_idx: None,                 //< value of previous lora symbol
                symbol_cnt: SyncState::NetId1, //< Number of symbols already received
                k_hat: 0,                      //< integer part of CFO+STO
                preamb_up_vals: vec![0; preamble_len_tmp - 3], //< value of the preamble upchirps
                frame_cnt: 0,                  //< Number of frame received
                m_symb_numb: 0,                //<number of payload lora symbols
                m_received_head: false, //< indicate that the header has be decoded and received by this block
                snr_est: 0.0,           //< estimate of the snr
                additional_upchirps: 0, //< indicate the number of additional upchirps found in preamble (in addition to the minimum required to trigger a detection)
                m_cfo_frac: 0.0,        //< fractional part of CFO
                sfo_hat: 0.0,           //< estimated sampling frequency offset
                sfo_cum: 0.0,           //< cumulation of the sfo
                cfo_frac_sto_frac_est: false, //< indicate that the estimation of CFO_frac and STO_frac has been performed
                down_val: None,               //< value of the preamble downchirps
                tag_from_msg_handler_to_work_channel: mpsc::channel::<Pmt>(1),
                known_valid_net_ids,
                known_valid_net_ids_reverse,
                net_id: [0; 2],
                ready_to_detect: true,
                net_id_caching_policy: net_id_caching_policy_tmp,
                collect_receive_statistics,
                receive_statistics_net_id_offset: 0,
                receive_statistics_one_symbol_off: false,
                fft_forward_number_of_bins: fft_detect,
                fft_forward_two_times_number_of_bins: FftPlanner::new()
                    .plan_fft(2 * m_number_of_bins_tmp, FftDirection::Forward),
                startup_timestamp_nanos: startup_timestamp
                    .unwrap_or(SystemTime::now())
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos() as u64,
                processed_samples: 0,
            },
        }
    }

    async fn poke(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        _p: Pmt,
    ) -> Result<Pmt> {
        io.call_again = true;
        Ok(Pmt::Null)
    }

    async fn bandwidth(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        if let Pmt::Usize(new_bw) = p {
            self.s.m_bw =
                Bandwidth::try_from(new_bw as u32).expect("received invalid bandwidth: {new_bw}");
            self.s.reset(false);
        } else {
            warn! {"PMT to bandwidth_handler was not a usize"}
        }
        Ok(Pmt::Null)
    }

    async fn center_freq(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        if let Pmt::Usize(new_center_freq) = p {
            self.s.m_center_freq = new_center_freq as u32;
            self.s.reset(false);
        } else {
            warn! {"PMT to center_freq_handler was not a usize"}
        }
        Ok(Pmt::Null)
    }

    async fn payload_crc_result(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        if let Pmt::Bool(crc_valid) = p {
            if self.s.net_id_caching_policy == NetIdCachingPolicy::PayloadCrcOk {
                // payload decoded successfully, cache current net_ids for future frame corrections
                if crc_valid {
                    self.s.cache_current_net_id();
                } else {
                    info!(
                        "failed to decode payload for netid [{}, {}], dropping.",
                        self.s.net_id[0], self.s.net_id[1]
                    );
                }
                self.s.ready_to_detect = true;
            }
        } else {
            warn!("payload_crc_result pmt was not a Bool");
        }
        Ok(Pmt::Null)
    }

    async fn frame_info(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        if let Pmt::MapStrPmt(mut frame_info) = p {
            let m_cr: usize = if let Pmt::Usize(temp) = frame_info.get("cr").unwrap() {
                *temp
            } else {
                panic!("invalid cr")
            };
            let m_pay_len: usize = if let Pmt::Usize(temp) = frame_info.get("pay_len").unwrap() {
                *temp
            } else {
                panic!("invalid pay_len")
            };
            let m_has_crc: bool = if let Pmt::Bool(temp) = frame_info.get("crc").unwrap() {
                *temp
            } else {
                panic!("invalid m_has_crc")
            };
            // uint8_t
            let ldro_mode_tmp: LdroMode =
                if let Pmt::Bool(temp) = frame_info.get("ldro_mode").unwrap() {
                    if *temp {
                        LdroMode::ENABLE
                    } else {
                        LdroMode::DISABLE
                    }
                } else {
                    panic!("invalid ldro_mode")
                };
            let m_invalid_header = if let Pmt::Bool(temp) = frame_info.get("err").unwrap() {
                *temp
            } else {
                panic!("invalid err flag")
            };

            debug!(
                "FrameSync: received header info: invalid header {m_invalid_header}, sf{}",
                self.s.m_sf
            );

            if m_invalid_header {
                self.s.reset(false)
            } else {
                // NetID caching
                if self.s.net_id_caching_policy == NetIdCachingPolicy::HeaderCrcOk {
                    self.s.cache_current_net_id();
                } else if self.s.net_id_caching_policy == NetIdCachingPolicy::PayloadCrcOk
                    && m_has_crc
                {
                    // blocking until crc result is in to avoid interleaving caching decision with next frame (with new unchecked net ids)
                    // TODO append NetID to FrameInfo, pass through to decoder, and include NetID in CRC result to avoid need to block
                    self.s.ready_to_detect = false;
                }
                // frame parameters
                let m_ldro: LdroMode = if ldro_mode_tmp == LdroMode::AUTO {
                    if (self.s.m_sf.samples_per_symbol()) as f32 * 1e3
                        / Into::<f32>::into(self.s.m_bw)
                        > LDRO_MAX_DURATION_MS
                    {
                        LdroMode::ENABLE
                    } else {
                        LdroMode::DISABLE
                    }
                } else {
                    ldro_mode_tmp
                };

                let m_symb_numb_tmp = 8_isize
                    + ((2 * m_pay_len as isize - self.s.m_sf as isize
                        + if self.s.m_sf >= SpreadingFactor::SF7 {
                            2
                        } else {
                            0
                        }
                        + (!self.s.m_impl_head) as isize * 5
                        + if m_has_crc { 4 } else { 0 }) as f64
                        / (Into::<usize>::into(self.s.m_sf) - 2 * m_ldro as usize) as f64)
                        .ceil() as isize
                        * (4 + m_cr as isize);
                assert!(
                    m_symb_numb_tmp >= 0,
                    "FrameSync::frame_info_handler computed negative symbol number"
                );
                self.s.m_symb_numb = m_symb_numb_tmp as usize;
                self.s.m_received_head = true;
                frame_info.insert(String::from("is_header"), Pmt::Bool(false));
                frame_info.insert(String::from("symb_numb"), Pmt::Usize(self.s.m_symb_numb));
                frame_info.remove("ldro_mode");
                frame_info.insert(String::from("ldro"), Pmt::Bool(m_ldro as usize != 0));
                let frame_info_pmt = Pmt::MapStrPmt(frame_info);
                self.s
                    .tag_from_msg_handler_to_work_channel
                    .0
                    .try_send(frame_info_pmt)
                    .unwrap();
            }
        } else {
            warn!("frame_info pmt was not a Map/Dict");
        }
        Ok(Pmt::Null)
    }
}

impl<I, O> Kernel for FrameSync<I, O>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let (out, mut out_tags) = self.output.slice_with_tags();
        let input = self.input.slice();
        let nitems_to_process = input.len();
        let out_len = out.len();

        let mut check_finished_or_abort = false;
        let consumed = if self.s.next_iteration_possible(nitems_to_process, out_len) {
            // downsampling
            // m_sto_frac is bounded by ]-0.5, 0.5] -> indexing_offset [0, self.m_os_factor[
            let indexing_offset = self.s.compute_sto_index();
            self.s.in_down = input
                .iter()
                .skip(indexing_offset)
                .step_by(self.s.m_os_factor)
                .take(self.s.m_number_of_bins)
                .copied()
                .collect();
            // outer state machine
            let (items_to_consume, items_to_output) = match self.s.m_state {
                DecoderState::Detect => {
                    if self.s.net_id_caching_policy == NetIdCachingPolicy::PayloadCrcOk
                        && !self.s.ready_to_detect
                    {
                        (0, 0)
                    } else {
                        assert!(
                            nitems_to_process
                                >= self.s.m_os_factor / 2 + self.s.m_samples_per_symbol
                        );
                        self.s.detect(input)
                    }
                }
                DecoderState::Sync => self.s.sync(input, out, &mut out_tags, nitems_to_process),
                DecoderState::SfoCompensation => self.s.compensate_sfo(out, &mut out_tags),
            };
            debug_assert!(
                items_to_consume >= 0,
                "tried to consume negative amount of samples ({items_to_consume})"
            );
            debug_assert!(
                nitems_to_process >= items_to_consume as usize,
                "tried to consume {items_to_consume} samples, \
                but input buffer only holds {nitems_to_process}."
            );
            if items_to_consume > 0 {
                self.s.processed_samples += items_to_consume as u64;
                self.input.consume(items_to_consume as usize);
            }
            if items_to_output > 0 {
                self.output.produce(items_to_output);
            }
            if self.s.next_iteration_possible(
                nitems_to_process - items_to_consume as usize,
                out_len - items_to_output,
            ) {
                // if next iteration is possible without external events, explicitly call again to avoid deadlocks
                io.call_again = true;
            } else {
                check_finished_or_abort = true;
            }
            items_to_consume as usize
        } else {
            check_finished_or_abort = true;
            0
        };
        if check_finished_or_abort
            && self.input.finished()
            && !self
                .s
                .next_iteration_possible(nitems_to_process - consumed, self.s.m_samples_per_symbol)
        {
            // appropriately propagate flowgraph termination
            io.finished = true;
        }
        Ok(())
    }
}
