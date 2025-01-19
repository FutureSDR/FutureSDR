use futuresdr::num_complex::Complex32;
use futuresdr::num_complex::Complex64;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::ItemTag;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageOutputs;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Result;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::Tag;
use futuresdr::runtime::TypedBlock;
use futuresdr::runtime::WorkIo;
use futuresdr::tracing::warn;
use rustfft::FftPlanner;
use std::collections::HashMap;
use std::sync::Arc;

use crate::utils::*;

#[allow(non_snake_case)]
pub fn bessel_I0(x: f64) -> f64 {
    // adapted from https://users.rust-lang.org/t/modified-bessel-function-of-the-first-kind-of-order-zero/80227/7
    let base = x * x / 4.;
    let mut addend = 1.;
    let mut sum = 1.;
    for j in 1.. {
        addend = addend * base / (j * j) as f64;
        let old = sum;
        sum += addend;
        if sum == old || !sum.is_finite() {
            break;
        }
    }
    sum
}

#[derive(futuresdr::Block)]
pub struct FftDemod {
    m_sf: usize,                 //< Spreading factor
    m_cr: usize,                 //< Coding rate
    m_soft_decoding: bool,       //< Hard/Soft decoding
    max_log_approx: bool,        //< use Max-log approximation in LLR formula
    m_ldro: bool,                //< use low datarate optimisation
    m_symb_numb: usize,          //< number of symbols in the frame
    m_samples_per_symbol: usize, //< Number of samples received per lora symbols

    // variable used to perform the FFT demodulation
    base_downchirp: Vec<Complex64>, //< Reference upchirp
    m_downchirp: Vec<Complex32>,    //< Reference downchirp
    output: Vec<u16>, //< Stores the value to be outputted once a full bloc has been received
    llrs_block: Vec<[LLR; MAX_SF]>, //< Stores the LLRs to be outputted once a full bloc has been received
    is_header: bool,                //< Indicate that the first block hasn't been fully received
    fft_plan: Arc<dyn rustfft::Fft<f32>>,
    // soft deoding buffers:
    lls: Vec<f64>,       // 2**sf  Log-Likelihood
    llrs: [LLR; MAX_SF], //      Log-Likelihood Ratios
}

impl FftDemod {
    pub fn new(soft_decoding: bool, sf_initial: usize) -> TypedBlock<Self> {
        let m_samples_per_symbol = 1_usize << sf_initial;
        let fft_plan = FftPlanner::new().plan_fft_forward(m_samples_per_symbol);
        let fs = Self {
            m_sf: sf_initial,
            m_cr: 0, // initial value irrelevant, set from tag before read
            m_soft_decoding: soft_decoding,
            max_log_approx: true,
            m_samples_per_symbol,
            base_downchirp: build_upchirp(0, sf_initial, 1)
                .iter()
                .map(|&x| Complex64::new(x.re as f64, x.im as f64).conj())
                .collect(),
            m_downchirp: vec![],
            output: Vec::with_capacity(8),
            llrs_block: Vec::with_capacity(8),
            is_header: false,
            m_ldro: false,
            m_symb_numb: 0,
            fft_plan,
            lls: vec![0.; m_samples_per_symbol],
            llrs: [0.; MAX_SF],
        };
        let mut sio = StreamIoBuilder::new().add_input::<Complex32>("in");
        if soft_decoding {
            sio = sio.add_output::<[LLR; MAX_SF]>("out")
        } else {
            sio = sio.add_output::<u16>("out")
        }
        TypedBlock::new(sio.build(), fs)
    }

    /// Set spreading factor and init vector sizes accordingly
    fn set_sf(&mut self, sf: usize) {
        //Set he new sf for the frame
        // info!("[fft_demod_impl.cc] new sf received {}", sf);
        self.m_sf = sf;
        self.m_samples_per_symbol = 1_usize << self.m_sf;
        self.fft_plan = FftPlanner::new().plan_fft_forward(self.m_samples_per_symbol);
        self.base_downchirp = build_upchirp(0, sf, 1)
            .iter()
            .map(|&x| Complex64::new(x.re as f64, x.im as f64).conj())
            .collect();
    }

    ///Compute the FFT and fill the class attributes
    fn compute_fft_mag(&self, samples: &[Complex32]) -> Vec<f64> {
        // Multiply with ideal downchirp
        let mut m_dechirped = volk_32fc_x2_multiply_32fc(samples, &self.m_downchirp);
        // do the FFT
        self.fft_plan.process(&mut m_dechirped);
        // Get magnitude squared
        let m_fft_mag_sq = volk_32fc_magnitude_squared_32f(&m_dechirped);
        m_fft_mag_sq.iter().map(|x| *x as f64).collect()
    }
    //
    /// Use in Hard-decoding
    /// Recover the lora symbol value using argmax of the dechirped symbol FFT.
    /// \param  samples
    ///         The pointer to the symbol beginning.
    fn get_symbol_val(&self, samples: &[Complex32]) -> u16 {
        let m_fft_mag_sq = self.compute_fft_mag(samples);
        // Return argmax
        let idx = argmax_f64(&m_fft_mag_sq);
        idx.try_into().unwrap()
    }

    /// check if the reduced rate should be used
    fn reduced_rate(&self) -> bool {
        if LEGACY_SF_5_6 {
            self.is_header || self.m_ldro
        } else {
            (self.is_header && self.m_sf >= 7) || (!self.is_header && self.m_ldro)
        }
    }

    ///  Use in Soft-decoding
    /// Compute the Log-Likelihood Ratios of the SF nbr of bits
    fn compute_llrs(&mut self, samples: &[Complex32]) {
        let mut m_fft_mag_sq = self.compute_fft_mag(samples);
        // compute SNR estimate at each received symbol as SNR remains constant during 1 simulation run
        // Estimate signal power
        let symbol_idx = argmax_f64(&m_fft_mag_sq);
        // Estimate noise power
        let mut signal_energy: f64 = 0.;
        let mut noise_energy: f64 = 0.;

        let n_adjacent_bins = 1; // Put '0' for best accurate SNR estimation but if symbols energy splitted in 2 bins, put '1' for safety
        for (i, &frequency_bin_energy) in m_fft_mag_sq.iter().enumerate() {
            if ((i as isize - symbol_idx as isize).unsigned_abs() % (self.m_samples_per_symbol - 1))
                < 1 + n_adjacent_bins
            {
                signal_energy += frequency_bin_energy;
            } else {
                noise_energy += frequency_bin_energy;
            }
        }

        // If you want to use a normalized constant identical to all symbols within a frame, but it leads to same performance
        // Lowpass filter update
        //double p = 0.99; // proportion to keep
        //Ps_est = p*Ps_est + (1-p)*  signal_energy / m_samples_per_symbol;
        //Pn_est = p*Pn_est + (1-p)* noise_energy / (m_samples_per_symbol-1-2*n_adjacent_bins); // remove used bins for better estimation
        // Signal and noise power estimation for each received symbol
        let m_ps_est = signal_energy / self.m_samples_per_symbol as f64;
        let m_pn_est = noise_energy / (self.m_samples_per_symbol - 1 - 2 * n_adjacent_bins) as f64;

        let _snr_db_estimate = 10. * (m_ps_est / m_pn_est).log10();
        // info!("SNR {}", SNRdB_estimate);
        //  Normalize fft_mag to 1 to avoid Bessel overflow
        m_fft_mag_sq = m_fft_mag_sq
            .iter()
            .map(|x| x * self.m_samples_per_symbol as f64)
            .collect();
        // upgrade to avoid for loop
        // Normalized |Y[n]| * sqrt(N) => |Y[n]|² * N (depends on kiss FFT library)
        //m_fft_mag_sq[i] /= Ps_frame; // // Normalize to avoid Bessel overflow (does not change the performances)
        //
        let mut clipping = false;
        #[allow(clippy::needless_range_loop)]
        for n in 0..self.m_samples_per_symbol {
            let bessel_arg = m_ps_est.sqrt() / m_pn_est * m_fft_mag_sq[n].sqrt();
            // Manage overflow of Bessel function
            // 713 ~ log(std::numeric_limits<LLR>::max())
            // if bessel_arg < 713. {
            // println!("{}", LLR::MAX.ln() as usize);
            if bessel_arg < 709. {
                // TODO original limit produces NaNs
                // TODO extremely slow implementation
                // let tmp = bessel::i_nu(0., Complex64::new(bessel_arg, 0.));
                let tmp = bessel_I0(bessel_arg);
                assert!(!tmp.is_nan());
                self.lls[n] = tmp; // compute Bessel safely
            } else {
                //std::cerr << RED << "Log-Likelihood clipping :-( SNR: " << SNRdB_estimate << " |Y|: " << std::sqrt(m_fft_mag_sq[n]) << RESET << std::endl;
                //LLs[n] = std::numeric_limits<LLR>::max();  // clipping
                clipping = true;
                break;
            }
            if self.max_log_approx {
                self.lls[n] = self.lls[n].ln(); // Log-Likelihood
                                                //LLs[n] = m_fft_mag_sq[n]; // same performance with just |Y[n]| or |Y[n]|²
            }
        }
        // change to max-log formula with only |Y[n]|² to avoid overflows, solve LLR computation incapacity in high SNR
        if clipping {
            self.lls.copy_from_slice(&m_fft_mag_sq);
        }

        // Log-Likelihood Ratio estimations
        if self.max_log_approx {
            for i in 0..self.m_sf {
                // sf bits => sf LLRs
                let mut max_x1: f64 = 0.;
                let mut max_x0: f64 = 0.; // X1 = set of symbols where i-th bit is '1'
                for (n, &ll) in self.lls.iter().enumerate() {
                    // for all symbols n : 0 --> 2^sf
                    // LoRa: shift by -1 and use reduce rate if first block (header)
                    let mut s: usize = my_modulo(n as isize - 1, 1 << self.m_sf)
                        / if self.reduced_rate() { 4 } else { 1 };
                    s = s ^ (s >> 1); // Gray encoding formula               // Gray demap before (in this block)
                    if (s & (1 << i)) != 0 {
                        // if i-th bit of symbol n is '1'
                        if ll > max_x1 {
                            max_x1 = ll
                        }
                    } else {
                        // if i-th bit of symbol n is '0'
                        if ll > max_x0 {
                            max_x0 = ll
                        }
                    }
                }
                self.llrs[self.m_sf - 1 - i] = max_x1 - max_x0; // [MSB ... ... LSB]
            }
        } else {
            // Without max-log approximation of the LLR estimation
            for i in 0..self.m_sf {
                let mut sum_x1: f64 = 0.;
                let mut sum_x0: f64 = 0.; // X1 = set of symbols where i-th bit is '1'
                for (n, &ll) in self.lls.iter().enumerate() {
                    // for all symbols n : 0 --> 2^sf
                    let mut s: usize =
                        ((n - 1) % (1 << self.m_sf)) / if self.reduced_rate() { 4 } else { 1 };
                    s = s ^ (s >> 1); // Gray demap
                    if (s & (1 << i)) != 0 {
                        sum_x1 += ll;
                    }
                    // Likelihood
                    else {
                        sum_x0 += ll;
                    }
                }
                self.llrs[self.m_sf - 1 - i] = sum_x1.ln() - sum_x0.ln();
                // [MSB ... ... LSB]
            }
        }
    }

    fn next_iteration_possible(
        &self,
        samples_left_in_input: usize,
        space_left_in_output: usize,
    ) -> bool {
        if samples_left_in_input < self.m_samples_per_symbol {
            return false;
        }
        let block_size = 4 + if self.is_header { 4 } else { self.m_cr };
        if self.m_soft_decoding {
            self.llrs_block.len() < block_size || space_left_in_output >= block_size
        } else {
            self.output.len() < block_size || space_left_in_output >= block_size
        }
    }
}

impl Kernel for FftDemod {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let input = sio.input(0).slice::<Complex32>();
        let mut nitems_to_process = input.len();
        let output_len = if self.m_soft_decoding {
            sio.output(0).slice::<[LLR; MAX_SF]>().len()
        } else {
            sio.output(0).slice::<u16>().len()
        };

        let tags: Vec<(usize, &HashMap<String, Pmt>)> = sio
            .input(0)
            .tags()
            .iter()
            .filter_map(|x| match x {
                ItemTag {
                    index,
                    tag: Tag::NamedAny(n, val),
                } => {
                    if n == "frame_info" {
                        match (**val).downcast_ref().unwrap() {
                            Pmt::MapStrPmt(map) => Some((*index, map)),
                            _ => None,
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect();

        let tag_tmp = if !tags.is_empty() {
            if tags[0].0 != 0 {
                nitems_to_process = nitems_to_process.min(tags[0].0);
                if nitems_to_process < self.m_samples_per_symbol {
                    warn!("FftDemod: incorrect number of samples; dropping.");
                    sio.input(0).consume(nitems_to_process);
                    io.call_again = true;
                    return Ok(());
                }
                None
            } else {
                if tags.len() >= 2 {
                    nitems_to_process = nitems_to_process.min(tags[1].0);
                    if nitems_to_process < self.m_samples_per_symbol {
                        warn!("FftDemod: too few samples between tags; dropping.");
                        sio.input(0).consume(nitems_to_process);
                        io.call_again = true;
                        return Ok(());
                    }
                }
                let (_, tag) = tags[0];
                self.is_header = if let Pmt::Bool(tmp) = tag.get("is_header").unwrap() {
                    *tmp
                } else {
                    panic!()
                };
                if self.is_header
                // new frame beginning
                {
                    // warn!("FftDemod: received new header tag.");
                    let cfo_int = if let Pmt::Isize(tmp) = tag.get("cfo_int").unwrap() {
                        *tmp
                    } else {
                        panic!()
                    };
                    let cfo_frac = if let Pmt::F64(tmp) = tag.get("cfo_frac").unwrap() {
                        *tmp
                    } else {
                        panic!()
                    };
                    let sf = if let Pmt::Usize(tmp) = tag.get("sf").unwrap() {
                        *tmp
                    } else {
                        panic!()
                    };
                    if sf != self.m_sf {
                        self.set_sf(sf);
                    }
                    // adapt the downchirp to the cfo_frac of the frame
                    self.m_downchirp = self
                        .base_downchirp
                        .iter()
                        .enumerate()
                        .map(|(i, &x)| {
                            x * Complex64::from_polar(
                                1.,
                                -2. * std::f64::consts::PI * (cfo_int as f64 + cfo_frac)
                                    / self.m_samples_per_symbol as f64
                                    * i as f64,
                            )
                        })
                        .map(|x| Complex32::new(x.re as f32, x.im as f32))
                        .collect();
                    if self.m_soft_decoding {
                        self.llrs_block.clear();
                    } else {
                        self.output.clear();
                    }
                } else {
                    self.m_cr = if let Pmt::Usize(tmp) = tag.get("cr").unwrap() {
                        *tmp
                    } else {
                        panic!()
                    };
                    // warn!("FftDemod: received new cr {}", self.m_cr);
                    self.m_ldro = if let Pmt::Bool(tmp) = tag.get("ldro").unwrap() {
                        *tmp
                    } else {
                        panic!()
                    };
                    self.m_symb_numb = if let Pmt::Usize(tmp) = tag.get("symb_numb").unwrap() {
                        *tmp
                    } else {
                        panic!()
                    };
                }
                Some(tag.clone())
            }
        } else {
            None
        };

        let block_size = 4 + if self.is_header { 4 } else { self.m_cr };

        let consumed = if self.next_iteration_possible(nitems_to_process, output_len) {
            if let Some(tag) = tag_tmp {
                sio.output(0).add_tag(
                    0,
                    Tag::NamedAny("frame_info".to_string(), Box::new(Pmt::MapStrPmt(tag))),
                );
            }
            if self.m_soft_decoding {
                self.compute_llrs(&input[..self.m_samples_per_symbol]);
                self.llrs_block.push(self.llrs); // Store 'sf' LLRs
            } else {
                // Hard decoding
                // shift by -1 and use reduce rate if first block (header)
                self.output.push(
                    my_modulo(
                        self.get_symbol_val(&input[..self.m_samples_per_symbol]) as isize - 1,
                        1 << self.m_sf,
                    ) as u16
                        / if self.reduced_rate() { 4 } else { 1 },
                );
            }

            let produced = if !self.m_soft_decoding && self.output.len() == block_size {
                let out_buf = sio.output(0).slice::<u16>();
                out_buf[0..block_size].copy_from_slice(&self.output);
                self.output.clear();
                block_size
            } else if self.m_soft_decoding && self.llrs_block.len() == block_size {
                let out_buf = sio.output(0).slice::<[LLR; MAX_SF]>();
                out_buf[0..block_size].copy_from_slice(&self.llrs_block);
                self.llrs_block.clear();
                block_size
            } else {
                0
            };

            // warn!("FftDemod: consumed {self.m_samples_per_symbol}");
            sio.input(0).consume(self.m_samples_per_symbol);
            if produced > 0 {
                sio.output(0).produce(produced);
            }

            io.call_again = self.next_iteration_possible(
                input.len() - self.m_samples_per_symbol,
                output_len - produced,
            );
            self.m_samples_per_symbol
        } else {
            0
        };

        if !io.call_again
            && sio.input(0).finished()
            && (input.len() - consumed) < self.m_samples_per_symbol
        {
            io.finished = true;
        }

        Ok(())
    }
}
