use futuresdr::num_complex::Complex32;
use rustfft::Fft;
use rustfft::FftPlanner;
use std::sync::Arc;

use crate::get_be_bit;
use crate::util::FROZEN_2048_1056;
use crate::util::FROZEN_2048_1392;
use crate::util::FROZEN_2048_712;
use crate::Mls;
use crate::OperationMode;
use crate::OrderedStatisticsDecoder;
use crate::Xorshift32;
use crate::PolarDecoder;

struct TheilSenEstimator {
    tmp: [f32; Self::SIZE],
    xint: f32,
    yint: f32,
    slope: f32,
}
impl TheilSenEstimator {
    const LEN_MAX: usize = 256;
    const SIZE: usize = ((Self::LEN_MAX-1) * Self::LEN_MAX) / 2;

    fn new() -> Self {
        Self {
            tmp: [0.0; Self::SIZE],
            xint: 0.0,
            yint: 0.0,
            slope: 0.0,
        }
    }

    fn compute(&mut self, x: &[f32], y: &[f32], len: usize) {
        let mut count = 0;
        let mut i = 0;
        while count < Self::SIZE && i < len {
            let mut j = i + 1;
            while count < Self::SIZE && j < len {
                if x[j] != x[i] {
                    self.tmp[count] = (y[j] - y[i]) / (x[j] - x[i]);
                    count += 1;
                }
                j += 1;
            }
            i += 1;
        }
        self.tmp[0..count].sort_by(|a, b| a.partial_cmp(b).unwrap());
        self.slope = self.tmp[count/2];
        count = 0;
        let mut i = 0;
        while count < Self::SIZE && i < len {
            self.tmp[count] = y[i] - self.slope * x[i];
            count += 1;
            i += 1;
        }
        self.tmp[0..count].sort_by(|a, b| a.partial_cmp(b).unwrap());
        self.yint = self.tmp[count/2];
        self.xint = - self.yint / self.slope;
    }

	fn get(&self, x: f32) -> f32 {
		self.yint + self.slope * x
	}
}

struct BoseChaudhuriHocquenghemGenerator;

impl BoseChaudhuriHocquenghemGenerator {
    const N: usize = 255;
    const K: usize = 71;
    const NP: usize = 255 - 71;

    fn poly(genpoly: &mut [i8], minimal_polynomials: &[i64]) {
        let mut gen_poly_degree = 1;
        genpoly.fill(0);
        genpoly[Self::NP] = 1;

        for m in minimal_polynomials.iter().copied() {
            assert!(0 < m);
            assert!(m & 1 == 1);
            let mut m_degree = 0;
            while (m >> m_degree) != 0 {
                m_degree += 1;
            }
            m_degree -= 1;
            assert!(gen_poly_degree + m_degree <= Self::NP + 1);
            for i in (0..=gen_poly_degree).rev() {
                if genpoly[Self::NP - i] == 0 {
                    continue;
                }
                genpoly[Self::NP - i] = (m & 1) as i8;
                for j in 1..=m_degree {
                    genpoly[Self::NP - (i + j)] ^= ((m >> j) & 1) as i8;
                }
            }
            gen_poly_degree += m_degree;
        }
        assert!(gen_poly_degree == Self::NP + 1);
        assert!(genpoly[0] != 0);
        assert!(genpoly[Self::NP] != 0);
    }

    fn matrix(genmat: &mut [i8], systematic: bool, minimal_polynomials: &[i64]) {
        Self::poly(genmat, minimal_polynomials);
        for i in Self::NP + 1..Self::N {
            genmat[i] = 0;
        }
        for j in 1..Self::K {
            for i in 0..j {
                genmat[Self::N * j + i] = 0;
            }
            for i in 0..=Self::NP {
                genmat[(Self::N + 1) * j + i] = genmat[i];
            }
            for i in (j + Self::NP + 1)..Self::N {
                genmat[Self::N * j + i] = 0
            }
        }
        if systematic {
            for k in (1..Self::K).rev() {
                for j in 0..k {
                    if genmat[Self::N * j + k] != 0 {
                        for i in k..Self::N {
                            genmat[Self::N * j + i] ^= genmat[Self::N * k + i];
                        }
                    }
                }
            }
        }
    }
}

struct Bpsk;

impl Bpsk {
    fn quantize(precision: f32, mut value: f32) -> i8 {
        value *= 2.0 * precision;
        value = value.round();
        value = value.max(-128.0).min(127.0);
        value as i8
    }

    fn soft(b: &mut i8, c: Complex32, precision: f32) {
        *b = Self::quantize(precision, c.re);
    }
}

struct Phasor {
    prev: Complex32,
    delta: Complex32,
}

impl Phasor {
    fn new() -> Self {
        Self {
            prev: Complex32::new(1.0, 0.0),
            delta: Complex32::new(1.0, 0.0),
        }
    }

    fn omega(&mut self, v: f32) {
        self.delta = Complex32::new(v.cos(), v.sin());
    }

    fn get(&mut self) -> Complex32 {
        let tmp = self.prev;
        self.prev *= self.delta;
        self.prev /= self.prev.norm();
        tmp
    }
}

struct SchmidlCox {
    tmp0: [Complex32; Self::SYMBOL_LEN],
    tmp1: [Complex32; Self::SYMBOL_LEN],
    tmp2: [Complex32; Self::SYMBOL_LEN],
    kern: [Complex32; Self::SYMBOL_LEN],
    fft_scratch: [Complex32; Self::SYMBOL_LEN],
    fft_fwd: Arc<dyn Fft<f32>>,
    fft_bwd: Arc<dyn Fft<f32>>,
    index_max: usize,
    symbol_pos: usize,
    timing_max: f32,
    phase_max: f32,
    cfo_rad: f32,
    frac_cfo: f32,
    cor: Sma4Complex32<{ Self::SYMBOL_LEN }, false>,
    pwr: Sma4F32<{ Self::SYMBOL_LEN * 2 }, false>,
    matc: Sma4F32<{ Self::MATCH_LEN }, false>,
    threshold: SchmittTrigger,
    falling: FallingEdgeTrigger,
    delay: Delay<{ Self::MATCH_DEL }>,
}

impl SchmidlCox {
    const RATE: usize = 48000;
    const SYMBOL_LEN: usize = (1280 * Self::RATE) / 8000 / 2;
    const GUARD_LEN: usize = Self::SYMBOL_LEN / 8 * 2;
    const EXTENDED_LEN: usize = Self::SYMBOL_LEN * 2 + Self::GUARD_LEN;
    const MATCH_LEN: usize = Self::GUARD_LEN | 1;
    const MATCH_DEL: usize = (Self::MATCH_LEN - 1) / 2;
    const SEARCH_POS: usize = Self::EXTENDED_LEN;
    const BUFFER_LEN: usize = Self::EXTENDED_LEN * 4;

    fn bin(carrier: isize) -> usize {
        (carrier + Self::SYMBOL_LEN as isize) as usize % Self::SYMBOL_LEN
    }

    fn demod_or_erase(curr: Complex32, prev: Complex32) -> Complex32 {
        if !(prev.norm_sqr() > 0.0) {
            return Complex32::new(0.0, 0.0);
        }
        let cons = curr / prev;
        if !(cons.norm_sqr() <= 4.0) {
            return Complex32::new(0.0, 0.0);
        }
        cons
    }

    fn new(mut sequence: [Complex32; Self::SYMBOL_LEN]) -> Self {
        let mut fft_planner = FftPlanner::new();
        let fft_bwd = fft_planner.plan_fft_inverse(Self::SYMBOL_LEN);
        let fft_fwd = fft_planner.plan_fft_forward(Self::SYMBOL_LEN);

        // println!("symbol len {}", Self::SYMBOL_LEN);
        // print!("cor seq ");
        // for i in 0..Self::SYMBOL_LEN {
        //     print!("{}, ", sequence[i].re);
        // }
        // println!();

        let mut kern = [Complex32::new(0.0, 0.0); Self::SYMBOL_LEN];
        let mut fft_scratch = [Complex32::new(0.0, 0.0); Self::SYMBOL_LEN];
        fft_fwd.process_outofplace_with_scratch(&mut sequence, &mut kern, &mut fft_scratch);

        for i in 0..Self::SYMBOL_LEN {
            kern[i] = kern[i].conj() / Self::SYMBOL_LEN as f32;
        }
        // println!("kern {:?}", &kern[0..10]);

        Self {
            tmp0: [Complex32::new(0.0, 0.0); Self::SYMBOL_LEN],
            tmp1: [Complex32::new(0.0, 0.0); Self::SYMBOL_LEN],
            tmp2: [Complex32::new(0.0, 0.0); Self::SYMBOL_LEN],
            index_max: 0,
            symbol_pos: Self::SEARCH_POS,
            timing_max: 0.0,
            phase_max: 0.0,
            cfo_rad: 0.0,
            frac_cfo: 0.0,
            fft_bwd,
            fft_fwd,
            kern,
            fft_scratch,
            cor: Sma4Complex32::new(),
            pwr: Sma4F32::new(),
            matc: Sma4F32::new(),
            threshold: SchmittTrigger::new(
                0.17 * Self::MATCH_LEN as f32,
                0.19 * Self::MATCH_LEN as f32,
            ),
            falling: FallingEdgeTrigger::new(),
            delay: Delay::new(),
        }
    }

    fn put(&mut self, samples: [Complex32; Self::BUFFER_LEN]) -> bool {
        let p = self.cor.put(
            samples[Self::SEARCH_POS + Self::SYMBOL_LEN]
                * samples[Self::SEARCH_POS + 2 * Self::SYMBOL_LEN].conj(),
        );
        let r = 0.5
            * self
                .pwr
                .put(samples[Self::SEARCH_POS + 2 * Self::SYMBOL_LEN].norm_sqr());
        let min_r = 0.0001 * Self::SYMBOL_LEN as f32;
        let r = r.max(min_r);
        let timing = self.matc.put(p.norm_sqr() / (r * r));
        let phase = self.delay.put(p.arg());

        let collect = self.threshold.put(timing);
        let process = self.falling.put(collect);

        if !collect && !process {
            return false;
        }

        if self.timing_max < timing {
            self.timing_max = timing;
            self.phase_max = phase;
            self.index_max = Self::MATCH_DEL;
        } else if self.index_max < Self::SYMBOL_LEN + Self::GUARD_LEN + Self::MATCH_DEL {
            self.index_max += 1;
        }

        if !process {
            return false;
        }

        self.frac_cfo = self.phase_max / Self::SYMBOL_LEN as f32;
        let mut osc = Phasor::new();
        osc.omega(self.frac_cfo);
        let test_pos = Self::SEARCH_POS - self.index_max;
        self.index_max = 0;
        self.timing_max = 0.0;
        for i in 0..Self::SYMBOL_LEN {
            self.tmp1[i] = samples[i + test_pos + Self::SYMBOL_LEN] * osc.get();
        }
        self.fft_fwd.process_outofplace_with_scratch(
            &mut self.tmp1,
            &mut self.tmp0,
            &mut self.fft_scratch,
        );
        for i in 0..Self::SYMBOL_LEN {
            self.tmp1[i] = Self::demod_or_erase(self.tmp0[i], self.tmp0[Self::bin(i as isize - 1)]);
        }
        self.fft_fwd.process_outofplace_with_scratch(
            &mut self.tmp1,
            &mut self.tmp0,
            &mut self.fft_scratch,
        );
        for i in 0..Self::SYMBOL_LEN {
            self.tmp0[i] *= self.kern[i];
        }
        self.fft_bwd.process_outofplace_with_scratch(
            &mut self.tmp0,
            &mut self.tmp2,
            &mut self.fft_scratch,
        );

        let mut shift = 0;
        let mut peak = 0.0;
        let mut next = 0.0;
        for i in 0..Self::SYMBOL_LEN {
            let power = self.tmp2[i].norm_sqr();
            if power > peak {
                next = peak;
                peak = power;
                shift = i;
            } else if power > next {
                next = power;
            }
        }
        if peak <= next * 4.0 {
            return false;
        }

        let pos_err = (self.tmp2[shift].arg() * Self::SYMBOL_LEN as f32 / std::f32::consts::TAU)
            .round() as isize;
        if pos_err.abs() > Self::GUARD_LEN as isize / 2 {
            return false;
        }
        assert!(test_pos as isize > pos_err);
        self.symbol_pos = (test_pos as isize - pos_err) as usize;

        self.cfo_rad =
            shift as f32 * std::f32::consts::TAU / Self::SYMBOL_LEN as f32 - self.frac_cfo;
        if self.cfo_rad >= std::f32::consts::PI {
            self.cfo_rad -= std::f32::consts::TAU;
        }
        true
    }
}

struct FallingEdgeTrigger {
    previous: bool,
}

impl FallingEdgeTrigger {
    fn new() -> Self {
        Self { previous: false }
    }

    fn put(&mut self, input: bool) -> bool {
        let tmp = self.previous;
        self.previous = input;
        tmp && !input
    }
}

struct SchmittTrigger {
    low: f32,
    high: f32,
    previous: bool,
}

impl SchmittTrigger {
    fn new(low: f32, high: f32) -> Self {
        Self {
            low,
            high,
            previous: false,
        }
    }

    fn put(&mut self, input: f32) -> bool {
        if self.previous {
            if input < self.low {
                self.previous = false;
            }
        } else {
            if input > self.high {
                self.previous = true;
            }
        }
        self.previous
    }
}

struct Delay<const NUM: usize> {
    buf: [f32; NUM],
    pos: usize,
}

impl<const NUM: usize> Delay<NUM> {
    fn new() -> Self {
        Self {
            buf: [0.0; NUM],
            pos: 0,
        }
    }

    fn put(&mut self, input: f32) -> f32 {
        let tmp = self.buf[self.pos];
        self.buf[self.pos] = input;
        self.pos += 1;
        if self.pos >= NUM {
            self.pos = 0;
        }
        tmp
    }
}

struct Sma4F32<const NUM: usize, const NORM: bool>
where
    [(); 2 * NUM]:,
{
    swa: SwaF32<NUM>,
}

impl<const NUM: usize, const NORM: bool> Sma4F32<NUM, NORM>
where
    [(); 2 * NUM]:,
{
    fn new() -> Self {
        Self {
            swa: SwaF32::new(0.0),
        }
    }

    fn put(&mut self, input: f32) -> f32 {
        if NORM {
            self.swa.put(input) / NUM as f32
        } else {
            self.swa.put(input)
        }
    }
}

struct Sma4Complex32<const NUM: usize, const NORM: bool>
where
    [(); 2 * NUM]:,
{
    swa: SwaComplex32<NUM>,
}

impl<const NUM: usize, const NORM: bool> Sma4Complex32<NUM, NORM>
where
    [(); 2 * NUM]:,
{
    fn new() -> Self {
        Self {
            swa: SwaComplex32::new(Complex32::new(0.0, 0.0)),
        }
    }

    fn put(&mut self, input: Complex32) -> Complex32 {
        if NORM {
            self.swa.put(input) / NUM as f32
        } else {
            self.swa.put(input)
        }
    }
}

struct SwaF32<const NUM: usize>
where
    [(); 2 * NUM]:,
{
    tree: [f32; 2 * NUM],
    leaf: usize,
}

impl<const NUM: usize> SwaF32<NUM>
where
    [(); 2 * NUM]:,
{
    pub fn new(ident: f32) -> Self {
        Self {
            tree: [ident; 2 * NUM],
            leaf: NUM,
        }
    }
    fn put(&mut self, input: f32) -> f32 {
        self.tree[self.leaf] = input;
        let mut child = self.leaf;
        let mut parent = self.leaf / 2;
        while parent > 0 {
            self.tree[parent] = self.tree[child] + self.tree[child ^ 1];
            child = parent;
            parent /= 2;
        }

        self.leaf += 1;
        if self.leaf >= 2 * NUM {
            self.leaf = NUM;
        }
        self.tree[1]
    }
}

struct SwaComplex32<const NUM: usize>
where
    [(); 2 * NUM]:,
{
    tree: [Complex32; 2 * NUM],
    leaf: usize,
}

impl<const NUM: usize> SwaComplex32<NUM>
where
    [(); 2 * NUM]:,
{
    pub fn new(ident: Complex32) -> Self {
        Self {
            tree: [ident; 2 * NUM],
            leaf: NUM,
        }
    }
    fn put(&mut self, input: Complex32) -> Complex32 {
        self.tree[self.leaf] = input;
        let mut child = self.leaf;
        let mut parent = self.leaf / 2;
        while parent > 0 {
            self.tree[parent] = self.tree[child] + self.tree[child ^ 1];
            child = parent;
            parent /= 2;
        }

        self.leaf += 1;
        if self.leaf >= 2 * NUM {
            self.leaf = NUM;
        }
        self.tree[1]
    }
}

struct BipBuffer<const NUM: usize>
where
    [(); 2 * NUM]:,
{
    buf: [Complex32; 2 * NUM],
    pos0: usize,
    pos1: usize,
}

impl<const NUM: usize> BipBuffer<NUM>
where
    [(); 2 * NUM]:,
{
    pub fn new() -> Self {
        Self {
            buf: [Complex32::new(0.0, 0.0); 2 * NUM],
            pos0: 0,
            pos1: NUM,
        }
    }
    pub fn get(&self) -> [Complex32; NUM] {
        let index = std::cmp::min(self.pos0, self.pos1);
        let mut buf = [Complex32::new(0.0, 0.0); NUM];
        buf.copy_from_slice(&self.buf[index..index + NUM]);
        buf
    }
    pub fn put(&mut self, input: Complex32) -> [Complex32; NUM] {
        self.buf[self.pos0] = input;
        self.buf[self.pos1] = input;
        self.pos0 += 1;
        if self.pos0 >= 2 * NUM {
            self.pos0 = 0;
        }
        self.pos1 += 1;
        if self.pos1 >= 2 * NUM {
            self.pos1 = 0;
        }
        self.get()
    }
}

#[derive(Clone, PartialEq)]
struct Kahan {
    high: f32,
    low: f32,
}

impl Kahan {
    pub fn new(init: f32) -> Self {
        Self {
            high: init,
            low: 0.0,
        }
    }

    fn same(&mut self, input: f32) -> bool {
        let tmp = self.clone();
        self.process(input);
        &tmp == self
    }

    fn process(&mut self, input: f32) -> f32 {
        let tmp = input - self.low;
        let sum = self.high + tmp;
        self.low = (sum - self.high) - tmp;
        self.high = sum;
        sum
    }

    fn get(&self) -> f32 {
        self.high
    }
}

pub struct Kaiser {
    a: f32,
}

impl Kaiser {
    fn i0(x: f32) -> f32 {
        let mut sum = Kahan::new(1.0);
        let mut val = 1.0;

        for n in 1..35 {
            val *= x / (2 * n) as f32;
            if sum.same(val * val) {
                return sum.get();
            }
        }
        sum.get()
    }

    fn new(a: f32) -> Self {
        Self { a }
    }

    fn get(&self, n: usize, nn: usize) -> f32 {
        let a = Self::i0(
            std::f32::consts::PI
                * self.a
                * (1.0 - ((2 * n) as f32 / (nn - 1) as f32 - 1.0).powi(2)).sqrt(),
        );
        let b = Self::i0(std::f32::consts::PI * self.a);
        // println!("kaiser n {} N {} ret {}", n, nn, a / b);
        a / b
    }
}

struct Hilbert<const TAPS: usize>
where
    [(); (TAPS - 1) / 4]:,
{
    real: [f32; TAPS],
    imco: [f32; (TAPS - 1) / 4],
    reco: f32,
}

impl<const TAPS: usize> Hilbert<TAPS>
where
    [(); (TAPS - 1) / 4]:,
{
    pub fn new() -> Self {
        assert_eq!((TAPS - 1) % 4, 0, "TAPS-1 not divisible by four");
        let kaiser = Kaiser::new(2.0);
        let reco = kaiser.get((TAPS - 1) / 2, TAPS);
        let real = [0.0; TAPS];
        let mut imco = [0.0; (TAPS - 1) / 4];

        for i in 0..(TAPS - 1) / 4 {
            imco[i] = kaiser.get((2 * i + 1) + (TAPS - 1) / 2, TAPS) * 2.0
                / ((2 * i + 1) as f32 * std::f32::consts::PI);
        }

        // println!("reco {}", reco);
        // println!("imco {:?}", imco);

        Self { real, imco, reco }
    }

    pub fn get(&mut self, input: f32) -> Complex32 {
        let re = self.reco * self.real[(TAPS - 1) / 2];
        let mut im = self.imco[0] * (self.real[(TAPS - 1) / 2 - 1] - self.real[(TAPS - 1) / 2 + 1]);
        for i in 1..(TAPS - 1) / 4 {
            im += self.imco[i]
                * (self.real[(TAPS - 1) / 2 - (2 * i + 1)]
                    - self.real[(TAPS - 1) / 2 + (2 * i + 1)]);
        }
        for i in 0..TAPS - 1 {
            self.real[i] = self.real[i + 1];
        }
        self.real[TAPS - 1] = input;

        // println!("hilbert input {} output {}", input, Complex32::new(re, im));
        Complex32::new(re, im)
    }
}

pub struct BlockDc {
    x1: f32,
    y1: f32,
    a: f32,
    b: f32,
}

impl BlockDc {
    pub fn new() -> Self {
        Self {
            x1: 0.0,
            y1: 0.0,
            a: 0.0,
            b: 0.5,
        }
    }

    fn samples(&mut self, s: usize) {
        self.a = (s - 1) as f32 / s as f32;
        self.b = (1.0 + self.a) / 2.0;
    }

    pub fn process(&mut self, x0: f32) -> f32 {
        let y0 = self.b * (x0 - self.x1) + self.a * self.y1;
        self.x1 = x0;
        self.y1 = y0;
        // println!("blockdc input {} output {}", x0, y0);
        y0
    }
}

#[derive(PartialEq)]
pub enum DecoderResult {
    Okay,
    Fail,
    Sync,
    Done,
    Heap,
    Nope,
    Ping,
    Failed,
    Preamble,
    Payload,
}

pub struct Decoder {
    block_dc: BlockDc,
    hilbert: Hilbert<{ Self::FILTER_LENGTH }>,
    buffer: BipBuffer<{ Self::BUFFER_LENGTH }>,
    correlator: SchmidlCox,
    symbol_number: isize,
    symbol_position: usize,
    stored_position: usize,
    staged_position: usize,
    staged_mode: OperationMode,
    operation_mode: OperationMode,
    accumulated: usize,
    stored_cfo_rad: f32,
    staged_cfo_rad: f32,
    staged_call: usize,
    stored_check: bool,
    staged_check: bool,
    osc: Phasor,
    fft_fwd: Arc<dyn Fft<f32>>,
    polar: PolarDecoder,
    buf: [Complex32; Self::BUFFER_LENGTH],
    temp: [Complex32; Self::EXTENDED_LENGTH],
    freq: [Complex32; Self::SYMBOL_LENGTH],
    fft_scratch: [Complex32; Self::SYMBOL_LENGTH],
    soft: [i8; Self::PRE_SEQ_LEN],
    generator: [i8; 255 * 71],
    code: [i8; Self::CODE_LEN],
    osd: OrderedStatisticsDecoder,
    data: [u8; (Self::PRE_SEQ_LEN + 7) / 8],
    cons: [Complex32; Self::PAY_CAR_CNT],
    prev: [Complex32; Self::PAY_CAR_CNT],
    index: [f32; Self::PAY_CAR_CNT],
    phase: [f32; Self::PAY_CAR_CNT],
    tse: TheilSenEstimator,
}

impl Decoder {
    const RATE: usize = 48000;
    const SYMBOL_LENGTH: usize = (1280 * Self::RATE) / 8000;
    const GUARD_LENGTH: usize = Self::SYMBOL_LENGTH / 8;
    const EXTENDED_LENGTH: usize = Self::SYMBOL_LENGTH + Self::GUARD_LENGTH;
    const FILTER_LENGTH: usize = (((33 * Self::RATE) / 8000) & !3) | 1;
    const COR_SEQ_POLY: u64 = 0b10001001;
    const COR_SEQ_LEN: usize = 127;
    const COR_SEQ_OFF: isize = 1 - Self::COR_SEQ_LEN as isize;
    const SEARCH_POSITION: usize = Self::EXTENDED_LENGTH;
    const SYMBOL_COUNT: usize = 4;
    const MOD_BITS: usize = 2;
    const BUFFER_LENGTH: usize = Self::EXTENDED_LENGTH * 4;
    const CODE_LEN: usize = 1 << 11;
    const PRE_SEQ_LEN: usize = 255;
    const PRE_SEQ_OFF: isize = -(Self::PRE_SEQ_LEN as isize) / 2;
    const PRE_SEQ_POLY: u64 = 0b100101011;
	const PAY_CAR_CNT: usize = 256;
	const PAY_CAR_OFF: isize = -(Self::PAY_CAR_CNT as isize) / 2;
    const CRC: crc::Crc<u16> = crc::Crc::<u16>::new(&crc::Algorithm {
        width: 16,
        poly: 0x2F15,
        init: 0x0000,
        refin: true,
        refout: true,
        xorout: 0x0000,
        check: 0x0000,
        residue: 0x0000,
    });

    fn nrz(bit: bool) -> Complex32 {
        if bit {
            Complex32::new(-1.0, 0.0)
        } else {
            Complex32::new(1.0, 0.0)
        }
    }

    fn cor_seq() -> [Complex32; Self::SYMBOL_LENGTH / 2] {
        println!(
            "symbol {} len {} offset {}",
            Self::SYMBOL_LENGTH,
            Self::COR_SEQ_LEN,
            Self::COR_SEQ_OFF
        );
        let mut freq = [Complex32::new(0.0, 0.0); Self::SYMBOL_LENGTH / 2];
        let mut mls = Mls::new(Self::COR_SEQ_POLY);
        for i in 0..Self::COR_SEQ_LEN as isize {
            let index = (i + Self::COR_SEQ_OFF as isize / 2 + Self::SYMBOL_LENGTH as isize / 2)
                as usize
                % (Self::SYMBOL_LENGTH / 2);
            let seq = mls.next();
            // println!("i {} index {} seq {}", i, index, seq as u8);
            freq[index] = Self::nrz(seq);
        }
        freq
    }

    fn base37(str: &mut [u8], mut val: usize, len: usize) {
        let mut i = len as isize - 1;
        while i >= 0 {
            str[i as usize] = b" 0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"[val % 37];
            i -= 1;
            val /= 37;
        }
    }

    fn bin(carrier: isize) -> usize {
        (carrier + Self::SYMBOL_LENGTH as isize) as usize % Self::SYMBOL_LENGTH
    }

    fn demod_or_erase(curr: Complex32, prev: Complex32) -> Complex32 {
        if prev.norm_sqr() <= 0.0 {
            return Complex32::new(0.0, 0.0);
        }
        let cons = curr / prev;
        if cons.norm_sqr() > 4.0 {
            return Complex32::new(0.0, 0.0);
        }
        cons
    }

    pub fn new() -> Self {
        let mut block_dc = BlockDc::new();
        block_dc.samples(Self::FILTER_LENGTH);

        let mut fft_planner = FftPlanner::new();
        let fft_fwd = fft_planner.plan_fft_forward(Self::SYMBOL_LENGTH);

        let mut generator = [0; 255 * 71];
        BoseChaudhuriHocquenghemGenerator::matrix(
            &mut generator,
            true,
            &[
                0b100011101,
                0b101110111,
                0b111110011,
                0b101101001,
                0b110111101,
                0b111100111,
                0b100101011,
                0b111010111,
                0b000010011,
                0b101100101,
                0b110001011,
                0b101100011,
                0b100011011,
                0b100111111,
                0b110001101,
                0b100101101,
                0b101011111,
                0b111111001,
                0b111000011,
                0b100111001,
                0b110101001,
                0b000011111,
                0b110000111,
                0b110110001,
            ],
        );

        Self {
            block_dc,
            hilbert: Hilbert::new(),
            buffer: BipBuffer::new(),
            correlator: SchmidlCox::new(Self::cor_seq()),
            symbol_number: Self::SYMBOL_COUNT as isize,
            symbol_position: Self::SEARCH_POSITION + Self::EXTENDED_LENGTH,
            stored_position: 0,
            staged_position: 0,
            staged_mode: OperationMode::Null,
            operation_mode: OperationMode::Null,
            accumulated: 0,
            stored_cfo_rad: 0.0,
            staged_cfo_rad: 0.0,
            staged_call: 0,
            stored_check: false,
            staged_check: false,
            buf: [Complex32::new(0.0, 0.0); Self::BUFFER_LENGTH],
            temp: [Complex32::new(0.0, 0.0); Self::EXTENDED_LENGTH],
            freq: [Complex32::new(0.0, 0.0); Self::SYMBOL_LENGTH],
            fft_scratch: [Complex32::new(0.0, 0.0); Self::SYMBOL_LENGTH],
            soft: [0; Self::PRE_SEQ_LEN],
            generator,
            osc: Phasor::new(),
            fft_fwd,
            polar: PolarDecoder::new(),
            code: [0; Self::CODE_LEN],
            osd: OrderedStatisticsDecoder::new(),
            data: [0; (Self::PRE_SEQ_LEN + 7) / 8],
            cons: [Complex32::new(0.0, 0.0); Self::PAY_CAR_CNT],
            prev: [Complex32::new(0.0, 0.0); Self::PAY_CAR_CNT],
            index: [0.0; Self::PAY_CAR_CNT],
            phase: [0.0; Self::PAY_CAR_CNT],
            tse: TheilSenEstimator::new(),
        }
    }

    pub fn feed(&mut self, samples: &[f32]) -> bool {
        static mut RUN: usize = 0;

        let sample_count = samples.len();
        unsafe {
            // print!("sample count {}   run {} ", sample_count, RUN);
            RUN += 1;
        }
        assert!(sample_count <= Self::EXTENDED_LENGTH);

        for i in 0..sample_count {
            let b = self
                .buffer
                .put(self.hilbert.get(self.block_dc.process(samples[i])));
            if i == 0 {
                // println!("  buffer {:?}", &b[0..2]);
            }
            let c = self.correlator.put(b);
            if c {
                self.stored_cfo_rad = self.correlator.cfo_rad;
                self.stored_position = self.correlator.symbol_pos + self.accumulated;
                self.stored_check = true;
                println!("cfo {}, pos {}", self.stored_cfo_rad, self.stored_position);
            }
            self.accumulated += 1;
            if self.accumulated == Self::EXTENDED_LENGTH {
                self.buf = self.buffer.get();
            }
        }

        if self.accumulated >= Self::EXTENDED_LENGTH {
            self.accumulated -= Self::EXTENDED_LENGTH;
            if self.stored_check {
                self.staged_cfo_rad = self.stored_cfo_rad;
                self.staged_position = self.stored_position;
                self.staged_check = true;
                self.stored_check = false;
            }
            true
        } else {
            false
        }
    }

    pub fn process(&mut self) -> DecoderResult {
        let mut status = DecoderResult::Okay;

        if self.staged_check {
            self.staged_check = false;
            status = self.preamble();
            if status == DecoderResult::Okay {
                self.operation_mode = self.staged_mode;
                self.osc.omega(-self.staged_cfo_rad);
                self.symbol_position = self.staged_position;
                self.symbol_number = -1;
                status = DecoderResult::Sync;
            }
        }

        if self.symbol_number < Self::SYMBOL_COUNT as isize {
            for i in 0..Self::EXTENDED_LENGTH {
                self.temp[i] = self.buf[self.symbol_position + i] * self.osc.get();
            }
            self.fft_fwd.process_outofplace_with_scratch(&mut self.temp[0..Self::SYMBOL_LENGTH], &mut self.freq, &mut self.fft_scratch);
            if self.symbol_number >= 0 {
                for i in 0..Self::PAY_CAR_CNT {
                    self.cons[i] = Self::demod_or_erase(self.freq[Self::bin(i as isize + Self::PAY_CAR_OFF)], self.prev[i]);
                }
                self.compensate();
                self.demap();
            }
            self.symbol_number += 1;
            if self.symbol_number == Self::SYMBOL_COUNT as isize {
                status = DecoderResult::Done;
            }
            for i in 0..Self::PAY_CAR_CNT {
                self.prev[i] = self.freq[Self::bin(i as isize + Self::PAY_CAR_OFF)];
            }
        }

        status
    }

    fn preamble(&mut self) -> DecoderResult {
        let mut nco = Phasor::new();
        nco.omega(-self.staged_cfo_rad);
        for i in 0..Self::SYMBOL_LENGTH {
            self.temp[i] = self.buf[self.staged_position + i] * nco.get();
        }

        self.fft_fwd.process_outofplace_with_scratch(
            &mut self.temp[0..Self::SYMBOL_LENGTH],
            &mut self.freq,
            &mut self.fft_scratch,
        );

        let mut seq = Mls::new(Self::PRE_SEQ_POLY);
        for i in 00..Self::PRE_SEQ_LEN {
            self.freq[Self::bin(i as isize + Self::PRE_SEQ_OFF)] *= Self::nrz(seq.next());
        }

        for i in 0..Self::PRE_SEQ_LEN {
            Bpsk::soft(
                &mut self.soft[i],
                Self::demod_or_erase(
                    self.freq[Self::bin(i as isize + Self::PRE_SEQ_OFF)],
                    self.freq[Self::bin(i as isize - 1 + Self::PRE_SEQ_OFF)],
                ),
                32.0,
            );
        }

        if !self
            .osd
            .process(&mut self.data, &self.soft, &self.generator)
        {
            return DecoderResult::Fail;
        }

        let mut md = 0u64;
        for i in 0..55 {
            md |= if get_be_bit(&self.data, i) { 1 << i } else { 0 };
        }
        let mut cs = 0;
        for i in 0..16 {
            cs |= if get_be_bit(&self.data, i + 55) { 1 << i } else { 0 };
        }

        if Self::CRC.checksum(&(md << 9).to_le_bytes()) != cs {
            return DecoderResult::Fail;
        }

        self.staged_mode = (md & 0xff).into();
        self.staged_call = (md >> 8) as usize;

        if self.staged_mode == OperationMode::Null {
            return DecoderResult::Nope;
        }
        if self.staged_call == 0 || self.staged_call >= 129961739795077 {
            self.staged_call = 0;
            return DecoderResult::Nope;
        }

        // Todo status PING
        DecoderResult::Okay
    }

    pub fn staged(&self, cfo: &mut f32, mode: &mut OperationMode, call: &mut [u8]) {
        *cfo = self.staged_cfo_rad * (Self::RATE as f32 / std::f32::consts::TAU);
        *mode = self.staged_mode;
        Self::base37(call, self.staged_call, 9);
    }

    pub fn fetch(&mut self, payload: &mut [u8]) -> i32 {
        let (data_bits, frozen_bits) = match self.operation_mode {
            OperationMode::Null => return -1,
            OperationMode::Mode14 => (1360, &FROZEN_2048_1392),
            OperationMode::Mode15 => (1024, &FROZEN_2048_1056),
            OperationMode::Mode16 => (680, &FROZEN_2048_712),
        };
        let result = self.polar.decode(payload, &self.code, frozen_bits, data_bits);
        let mut scrambler = Xorshift32::new();
        for i in 0..data_bits / 8 {
            payload[i] ^= scrambler.next() as u8;
        }
        for i in data_bits / 8..170 {
            payload[i] = 0;
        }
        result
    }

    fn compensate(&mut self) {
        let mut count = 0;
        for i in 0..Self::PAY_CAR_CNT {
            let con = self.cons[i];
            if con.re != 0.0 && con.im != 0.0 {
                let mut tmp = [0i8; Self::MOD_BITS];
                Self::mod_hard(&mut tmp, con);
                self.index[count] = (i as isize + Self::PAY_CAR_OFF) as f32; 
                self.phase[count] = (con * Self::mod_map(&tmp).conj()).arg();
                count += 1;
            }
        }

        self.tse.compute(&self.index, &self.phase, count);
        for i in 0..Self::PAY_CAR_CNT {
            self.cons[i] *= Complex32::from_polar(1.0, - self.tse.get((i as isize + Self::PAY_CAR_OFF) as f32));
        }
    }

    fn mod_hard(b: &mut [i8], c: Complex32) {
		b[0] = if c.re < 0.0 { -1 } else { 1 };
		b[1] = if c.im < 0.0 { -1 } else { 1 };
    }

    fn mod_map(b: &[i8]) -> Complex32 {
        std::f32::consts::FRAC_1_SQRT_2 * Complex32::new(b[0] as f32, b[1] as f32)
	}

    fn precision(&self) -> f32 {
        let mut sp = 0.0;
        let mut np = 0.0;
        for i in 0..Self::PAY_CAR_CNT {
            let mut tmp = [0i8; Self::MOD_BITS];
            Self::mod_hard(&mut tmp, self.cons[i]);
            let hard = Self::mod_map(&tmp);
            let error = self.cons[i] - hard;
            sp += hard.norm_sqr();
            np += error.norm_sqr();
        }
        sp / np
    }

    fn demap(&mut self) {
        let pre = self.precision();
        for i in 0..Self::PAY_CAR_CNT {
            Self::mod_soft(&mut self.code[Self::MOD_BITS * (self.symbol_number as usize * Self::PAY_CAR_CNT + i)..], self.cons[i], pre)
        }
    }

    fn quantize(precision: f32, mut value: f32) -> i8 {
        let dist = 2.0 * std::f32::consts::FRAC_1_SQRT_2;
        value *= dist * precision;
        value = value.round();
        value = value.max(-128.0).min(127.0);
        value as i8
    }

	fn mod_soft(b: &mut [i8], c: Complex32, precision: f32) {
		b[0] = Self::quantize(precision, c.re);
		b[1] = Self::quantize(precision, c.im);
    }
}


