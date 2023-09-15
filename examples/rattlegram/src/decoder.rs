
#[derive(Clone, PartialEq)]
struct Kahan {
	high: f32,
    low: f32,
}

impl Kahan {
    pub fn new(init: f32) -> Self {
        Self {
            high: init, low: 0.0
        }
    }

    fn same(&self, input: f32) -> bool {
        let mut tmp = self.clone();
        tmp.process(input);
        tmp == self
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
        Self::i0(std::f32::consts::PI * self.a * (1.0 - ((2 * n) as f32 / (nn - 1) as f32 - 1.0).powi(2)).powi(2)) / Self::i0(std::f32::consts::PI * self.a)
    }
}


struct Hilbert<const TAPS: usize> {
    real: [f32; TAPS],
    imco: [f32; (TAPS-1)/4],
    reco: f32,
}

impl<const TAPS: usize> Hilbert<N> {
    pub fn new() {
        assert_eq!((TAPS-1) % 4, 0, "TAPS-1 not divisible by four");

    }

}
	Hilbert(value_type a = value_type(2))
	{
		Kaiser<value_type> win(a);
		reco = win((TAPS-1)/2, TAPS);
		for (int i = 0; i < (TAPS-1)/4; ++i)
			imco[i] = win((2*i+1)+(TAPS-1)/2, TAPS) * 2 / ((2*i+1) * Const<value_type>::Pi());
		for (int i = 0; i < TAPS; ++i)
			real[i] = 0;
	}
	complex_type operator()(value_type input)
	{
		value_type re = reco * real[(TAPS-1)/2];
		value_type im = imco[0] * (real[(TAPS-1)/2-1] - real[(TAPS-1)/2+1]);
		for (int i = 1; i < (TAPS-1)/4; ++i)
			im += imco[i] * (real[(TAPS-1)/2-(2*i+1)] - real[(TAPS-1)/2+(2*i+1)]);
		for (int i = 0; i < TAPS-1; ++i)
			real[i] = real[i+1];
		real[TAPS-1] = input;
		return complex_type(re, im);
	}
};


pub struct BlockDc {
    x1: f32,
    y1: f32,
    a: f32,
    b: f32,
}

impl BlockDc {
    pub fn new() -> Self {
        Self {
            x1: 0.0, y1: 0.0, a: 0.0, b: 0.5,
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
}

impl Decoder {
    const RATE: usize = 48000;
    const SYMBOL_LENGTH: usize = (1280 * Self::RATE) / 8000;
    const GUARD_LENGTH: usize = Self::SYMBOL_LENGTH / 8;
    const EXTENDED_LENGTH: usize = Self::SYMBOL_LENGTH + Self::GUARD_LENGTH;
    const FILTER_LENGTH: usize = (((33 * Self::RATE) / 8000) & ~3) | 1;

    pub fn new() -> Self {
        Self {
            block_dc: BlockDc::new(),
        }
    }

    pub fn feed(mut self, samples: &[f32]) -> Result<(), ()> {
        let sample_count = samples.len();
		assert!(sample_count <= Self::EXTENDED_LENGTH);

        for i in 0..sample_count {

}

		for (int i = 0; i < sample_count; ++i) {
			if (correlator(buffer(convert(audio_buffer, channel_select, i)))) {
				stored_cfo_rad = correlator.cfo_rad;
				stored_position = correlator.symbol_pos + accumulated;
				stored_check = true;
			}
			if (++accumulated == extended_length)
				buf = buffer();
		}
		if (accumulated >= extended_length) {
			accumulated -= extended_length;
			if (stored_check) {
				staged_cfo_rad = stored_cfo_rad;
				staged_position = stored_position;
				staged_check = true;
				stored_check = false;
			}
			return true;
		}
		return false;
	}




        Ok(())
    }

    pub fn process(mut self) -> DecoderResult {
        DecoderResult::Failed
    }
}
