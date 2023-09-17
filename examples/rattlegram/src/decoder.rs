use futuresdr::num_complex::Complex32;
use rustfft::Fft;
use rustfft::FftPlanner;
use std::sync::Arc;

use crate::util::FROZEN_2048_1056;
use crate::util::FROZEN_2048_1392;
use crate::util::FROZEN_2048_712;
use crate::OperationMode;
use crate::Mls;
use crate::Xorshift32;

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
    const BUFFER_LEN: usize = Self::EXTENDED_LEN * 4 ;

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

        println!("symbol len {}", Self::SYMBOL_LEN);
        print!("cor seq ");
        for i in 0..Self::SYMBOL_LEN {
            print!("{}, ", sequence[i].re);
        }
        println!();

        let mut kern = [Complex32::new(0.0, 0.0); Self::SYMBOL_LEN];
        let mut fft_scratch = [Complex32::new(0.0, 0.0); Self::SYMBOL_LEN];
        fft_fwd.process_outofplace_with_scratch(&mut sequence, &mut kern, &mut fft_scratch);

        for i in 0..Self::SYMBOL_LEN {
            kern[i] = kern[i].conj() / Self::SYMBOL_LEN as f32;
        }
        println!("kern {:?}", &kern[0..10]);

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
        let mut tmp = self.clone();
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

    symbol_numer: usize,
    symbol_position: usize,
    stored_position: usize,
    staged_position: usize,
    staged_mode: usize,
    operation_mode: OperationMode,
    accumulated: usize,
    stored_cfo_rad: f32,
    staged_cfo_rad: f32,
    staged_call: usize,
    stored_check: bool,
    staged_check: bool,
    polar: PolarDecoder,
    buf: [Complex32; Self::BUFFER_LENGTH],
	code: [i8; Self::CODE_LEN];
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
    const BUFFER_LENGTH: usize = Self::EXTENDED_LENGTH * 4;
    const CODE_LEN: usize = 1 << 11;

    fn nrz(bit: bool) -> Complex32 {
        if bit {
            Complex32::new(-1.0, 0.0)
        } else {
            Complex32::new(1.0, 0.0)
        }
    }

    fn cor_seq() -> [Complex32; Self::SYMBOL_LENGTH / 2] {
        println!("symbol {} len {} offset {}", Self::SYMBOL_LENGTH, Self::COR_SEQ_LEN, Self::COR_SEQ_OFF);
        let mut freq = [Complex32::new(0.0, 0.0); Self::SYMBOL_LENGTH / 2];
        let mut mls = Mls::new(Self::COR_SEQ_POLY);
        for i in 0..Self::COR_SEQ_LEN as isize {
            let index = (i + Self::COR_SEQ_OFF as isize / 2 + Self::SYMBOL_LENGTH as isize / 2) as usize % (Self::SYMBOL_LENGTH / 2);
            let seq = mls.next();
            println!("i {} index {} seq {}", i, index, seq as u8);
            freq[index] = Self::nrz(seq);
        }
        freq
    }

    fn base37(str: &mut [u8], mut val: usize, len: usize) {
        let mut i = len - 1;
        while i >= 0 {
			str[i] = b" 0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"[val % 37];
            i -= 1;
            val /= 37;
        }
    }

    pub fn new() -> Self {
        let mut block_dc = BlockDc::new();
        block_dc.samples(Self::FILTER_LENGTH);

        Self {
            block_dc,
            hilbert: Hilbert::new(),
            buffer: BipBuffer::new(),
            correlator: SchmidlCox::new(Self::cor_seq()),
            symbol_numer: Self::SYMBOL_COUNT,
            symbol_position: Self::SEARCH_POSITION + Self::EXTENDED_LENGTH,
            stored_position: 0,
            staged_position: 0,
            staged_mode: 0,
            operation_mode: OperationMode::Fail,
            accumulated: 0,
            stored_cfo_rad: 0.0,
            staged_cfo_rad: 0.0,
            staged_call: 0,
            stored_check: false,
            staged_check: false,
            buf: [Complex32::new(0.0, 0.0); Self::BUFFER_LENGTH],
            polar: PolarDecoder::new(),
            code: [0; Self::CODE_LEN];
        }
    }

    pub fn feed(&mut self, samples: &[f32]) -> bool {
        static mut RUN: usize = 0;

        let sample_count = samples.len();
        unsafe {
            print!("sample count {}   run {} ", sample_count, RUN);
            RUN += 1;
        }
        assert!(sample_count <= Self::EXTENDED_LENGTH);

        for i in 0..sample_count {
            let b = self.buffer.put(self.hilbert.get(self.block_dc.process(samples[i])));
            if i == 0 {
                println!("  buffer {:?}", &b[0..2]);
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
        DecoderResult::Failed
    }

    pub fn staged(&self, cfo: &mut f32, mode: &mut usize, call: &mut [u8]) {
        *cfo = self.staged_cfo_rad * (Self::RATE as f32 / std::f32::consts::TAU);
        *mode = self.staged_mode;
        Self::base37(call, self.staged_call, 9);
    }

    pub fn fetch(&self, payload: &mut [u8]) -> isize {
        let (data_bits, frozen_bits) = match self.operation_mode {
            OperationMode::Null => return -1,
            OperationMode::Mode14 => (1360, FROZEN_2048_1392),
            OperationMode::Mode15 => (1024, FROZEN_2048_1056),
            OperationMode::Mode16 => (680, FROZEN_2048_712),
        };
        let result = self.polar.process(payload, code, frozen_bits, data_bits);
        let mut scramber = Xorshift32::new();
        for i in 0..data_bits/8 {
			payload[i] ^= scrambler.next();
        }
        for i in data_bits/8..170 {
            payload[i] = 0;
        }
        result
    }
}
