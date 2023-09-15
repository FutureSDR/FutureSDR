
pub struct Kaiser {
    a: f32,
}
	/*
	i0() implements the zero-th order modified Bessel function of the first kind:
	https://en.wikipedia.org/wiki/Bessel_function#Modified_Bessel_functions:_I%CE%B1,_K%CE%B1
	$I_\alpha(x) = i^{-\alpha} J_\alpha(ix) = \sum_{m=0}^\infty \frac{1}{m!\, \Gamma(m+\alpha+1)}\left(\frac{x}{2}\right)^{2m+\alpha}$
	$I_0(x) = J_0(ix) = \sum_{m=0}^\infty \frac{1}{m!\, \Gamma(m+1)}\left(\frac{x}{2}\right)^{2m} = \sum_{m=0}^\infty \left(\frac{x^m}{2^m\,m!}\right)^{2}$
	We obviously can't use the factorial here, so let's get rid of it:
	$= 1 + \left(\frac{x}{2 \cdot 1}\right)^2 + \left(\frac{x}{2 \cdot 1}\cdot \frac{x}{2 \cdot 2}\right)^2 + \left(\frac{x}{2 \cdot 1}\cdot \frac{x}{2 \cdot 2}\cdot \frac{x}{2 \cdot 3}\right)^2 + .. = 1 + \sum_{m=1}^\infty \left(\prod_{n=1}^m \frac{x}{2n}\right)^2$
	*/
	static TYPE i0(TYPE x)
	{
		Kahan<TYPE> sum(1.0);
		TYPE val = 1.0;
		// converges for -3*Pi:3*Pi in less than:
		// float: 25 iterations
		// double: 35 iterations
		for (int n = 1; n < 35; ++n) {
			val *= x / TYPE(2 * n);
			if (sum.same(val * val))
				return sum();
		}
		return sum();
	}
	static TYPE sqr(TYPE x)
	{
		return x * x;
	}
public:
	Kaiser(TYPE a) : a(a) {}
	TYPE operator () (int n, int N) const
	{
		return i0(Const<TYPE>::Pi() * a * sqrt(TYPE(1) - sqr(TYPE(2 * n) / TYPE(N - 1) - TYPE(1)))) / i0(Const<TYPE>::Pi() * a);
	}
};




struct Hilbert<const N: usize> {
    real: [f32; N],
    imco: [f32; (N-1)/4],
    reco: f32,
}

impl<const N: usize> Hilbert<N> {
    pub fn new() {

    }

}
	static_assert((TAPS-1) % 4 == 0, "TAPS-1 not divisible by four");
	typedef TYPE complex_type;
	typedef typename TYPE::value_type value_type;
	value_type real[TAPS];
	value_type imco[(TAPS-1)/4];
	value_type reco;
public:
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
