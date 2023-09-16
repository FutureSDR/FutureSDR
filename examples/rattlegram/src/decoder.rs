use futuresdr::anyhow;
use futuresdr::num_complex::Complex32;

struct FallingEdgeTrigger {
    previous: bool,
}

impl FallingEdgeTrigger {
    fn new() -> Self {
        Self {
            previous: false,
        }
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
    fn new() -> Self {
        let threshold = 1.0/3.0;
        Self {
            low: -threshold,
            high: threshold,previous: false
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
            buf: [0.0; NUM],pos: 0,
        }
    }

    fn put(mut self, input: f32) -> f32 {
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

impl<const NUM: usize, const NORM:bool> Sma4F32<NUM, NORM>
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
    pub fn get(&self) -> Complex32 {
        self.buf[std::cmp::min(self.pos0, self.pos1)]
    }
    pub fn put(&mut self, input: Complex32) -> Complex32 {
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

    fn same(&self, input: f32) -> bool {
        let mut tmp = self.clone();
        tmp.process(input);
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
        let sum = Kahan::new(1.0);
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
        Self::i0(
            std::f32::consts::PI
                * self.a
                * (1.0 - ((2 * n) as f32 / (nn - 1) as f32 - 1.0).powi(2)).powi(2),
        ) / Self::i0(std::f32::consts::PI * self.a)
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

    pub fn process(&mut self, sample: f32) -> f32 {
        let y0 = self.b * (sample - self.x1) + self.a * self.y1;
        self.x1 = sample;
        self.y1 = y0;
        y0
    }
}

pub enum DecoderResult {
    Failed,
    Preamble,
    Payload,
}

pub struct Decoder {
    block_dc: BlockDc,
    hilbert: Hilbert<{ Self::FILTER_LENGTH }>,
    buffer: BipBuffer<{ Self::EXTENDED_LENGTH }>,
}

impl Decoder {
    const RATE: usize = 48000;
    const SYMBOL_LENGTH: usize = (1280 * Self::RATE) / 8000;
    const GUARD_LENGTH: usize = Self::SYMBOL_LENGTH / 8;
    const EXTENDED_LENGTH: usize = Self::SYMBOL_LENGTH + Self::GUARD_LENGTH;
    const FILTER_LENGTH: usize = (((33 * Self::RATE) / 8000) & !3) | 1;

    pub fn new() -> Self {
        Self {
            block_dc: BlockDc::new(),
            hilbert: Hilbert::new(),
            buffer: BipBuffer::new(),
        }
    }

    pub fn feed(&mut self, samples: &[f32]) -> anyhow::Result<()> {
        let sample_count = samples.len();
        assert!(sample_count <= Self::EXTENDED_LENGTH);

        for i in 0..sample_count {
            let _c = self
                .buffer
                .put(self.hilbert.get(self.block_dc.process(samples[i])));
        }

        Ok(())
    }

    // 	for (int i = 0; i < sample_count; ++i) {
    // 		if (correlator(buffer(convert(audio_buffer, channel_select, i)))) {
    // 			stored_cfo_rad = correlator.cfo_rad;
    // 			stored_position = correlator.symbol_pos + accumulated;
    // 			stored_check = true;
    // 		}
    // 		if (++accumulated == extended_length)
    // 			buf = buffer();
    // 	}
    // 	if (accumulated >= extended_length) {
    // 		accumulated -= extended_length;
    // 		if (stored_check) {
    // 			staged_cfo_rad = stored_cfo_rad;
    // 			staged_position = stored_position;
    // 			staged_check = true;
    // 			stored_check = false;
    // 		}
    // 		return true;
    // 	}
    // 	return false;
    // }
    //
    //
    //

    pub fn process(&mut self) -> DecoderResult {
        DecoderResult::Failed
    }
}
