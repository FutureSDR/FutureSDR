use futuresdr::{num_complex::Complex32, num_integer::Roots};
use rustfft::Fft;
use std::sync::Arc;

struct Mls {
    poly: u64,
    test: u64,
    reg: u64,
}

impl Mls {
    fn new(poly: u64) -> Self {
        Self {
            poly,
            test: Self::hibit(poly) >> 1,
            reg: 1,
        }
    }

    fn hibit(mut n: u64) -> u64 {
        n |= n >> 1;
        n |= n >> 2;
        n |= n >> 4;
        n |= n >> 8;
        n |= n >> 16;
        n ^ (n >> 1)
    }

    fn reset(&mut self, r: Option<u64>) {
        self.reg = r.unwrap_or(1);
    }

    fn next(&mut self) -> bool {
        let fb = self.reg & self.test;
        self.reg <<= 1;
        self.reg ^= fb * self.poly;
        fb != 0
    }

    fn bad(&mut self, r: Option<u64>) -> bool {
        let r = r.unwrap_or(1);
        self.reg = r;
        let len = Self::hibit(self.poly) - 1;

        for i in 1..len {
            self.next();
            if self.reg == r {
                return true;
            }
        }

        self.next();
        self.reg != r
    }
}

pub struct Psk<const N: usize> {}

impl Psk<4> {
    fn map(b: &[bool; 2]) -> Complex32 {
        const A: f32 = std::f32::consts::FRAC_1_SQRT_2;

        match b {
            [true, true] => Complex32::new(A, A),
            [true, false] => Complex32::new(A, -A),
            [false, true] => Complex32::new(-A, A),
            [false, false] => Complex32::new(-A, -A),
        }
    }
}

enum OperationMode {
    Null,
    Mode14,
    Mode15,
    Mode16,
}

pub struct Encoder<const RATE: i64>
where
    [(); Self::SYMBOL_LENGTH]: Send,
    [(); Self::PAY_CAR_CNT]: Send,
    [(); Self::EXTENDED_LENGTH]: Send,
{
    temp: [Complex32; Self::EXTENDED_LENGTH],
    freq: [Complex32; Self::SYMBOL_LENGTH],
    prev: [Complex32; Self::PAY_CAR_CNT],
    mls: Mls,
    symbol_number: usize,
    code: [bool; Self::CODE_LEN],
    carrier_offset: u64,
    fft_scratch: [Complex32; Self::SYMBOL_LENGTH],
    fft: Arc<dyn Fft<Complex32>>,
}

impl<const RATE: i64> Encoder<RATE> {
    const CODE_ORDER: usize = 11;
    const MOD_BITS: usize = 2;
    const CODE_LEN: usize = 1 << Self::CODE_ORDER;
    const SYMBOL_COUNT: usize = 4;
    const SYMBOL_LENGTH: usize = (1280 * RATE) / 8000;
    const GUARD_LENGTH: usize = Self::SYMBOL_LENGTH / 8;
    const EXTENDED_LENGTH: usize = Self::SYMBOL_LENGTH + Self::GUARD_LENGTH;
    const MAX_BITS: usize = 1360;
    const COR_SEQ_LEN: i64 = 127;
    const COR_SEQ_OFF: i64 = 1 - Self::COR_SEQ_LEN;
    const COR_SEQ_POLY: i64 = 0b10001001;
    const PRE_SEQ_LEN: i64 = 255;
    const PRE_SEQ_OFF: i64 = -Self::PRE_SEQ_LEN / 2;
    const PRE_SEQ_POLY: i64 = 0b100101011;
    const PAY_CAR_CNT: usize = 256;
    const PAY_CAR_OFF: usize = -Self::PAY_CAR_CNT / 2;
    const FANCY_OFF: i64 = -(8 * 9 * 3) / 2;
    const NOISE_POLY: i64 = 0b100101010001;

    pub fn encode(
        &self,
        payload: &[u8],
        call_sign: &[u8],
        carrier_frequency: u64,
        noise_symbols: u64,
        fancy_header: bool,
    ) -> Vec<f32> {
        let len = payload.len();

        // 	void configure(const uint8_t *payload, const int8_t *call_sign, int carrier_frequency, int noise_symbols, bool fancy_header) final {
        // 		int len = 0;
        // 		while (len <= 128 && payload[len])
        // 			++len;
        // 		if (!len)
        // 			operation_mode = 0;
        // 		else if (len <= 85)
        // 			operation_mode = 16;
        // 		else if (len <= 128)
        // 			operation_mode = 15;
        // 		else
        // 			operation_mode = 14;
        // 		carrier_offset = (carrier_frequency * symbol_length) / RATE;
        // 		meta_data = (base37(call_sign) << 8) | operation_mode;
        // 		for (int i = 0; i < 9; ++i)
        // 			call[i] = 0;
        // 		for (int i = 0; i < 9 && call_sign[i]; ++i)
        // 			call[i] = base37_map(call_sign[i]);
        // 		symbol_number = 0;
        // 		count_down = 5;
        // 		fancy_line = 11 * fancy_header;
        // 		noise_count = noise_symbols;
        // 		for (int i = 0; i < guard_length; ++i)
        // 			guard[i] = 0;
        // 		const uint32_t *frozen_bits;
        // 		int data_bits;
        // 		switch (operation_mode) {
        // 			case 14:
        // 				data_bits = 1360;
        // 				frozen_bits = frozen_2048_1392;
        // 				break;
        // 			case 15:
        // 				data_bits = 1024;
        // 				frozen_bits = frozen_2048_1056;
        // 				break;
        // 			case 16:
        // 				data_bits = 680;
        // 				frozen_bits = frozen_2048_712;
        // 				break;
        // 			default:
        // 				return;
        // 		}
        // 		CODE::Xorshift32 scrambler;
        // 		for (int i = 0; i < data_bits / 8; ++i)
        // 			mesg[i] = payload[i] ^ scrambler();
        // 		polar(code, mesg, frozen_bits, data_bits);
        // 	}

        Vec::new()
    }

    pub fn rate() -> i64 {
        RATE
    }

    fn nrz(bit: bool) -> f32 {
        if bit {
            -1.0
        } else {
            1.0
        }
    }

    fn bin(self, carrier: u64) -> u64 {
        (carrier + self.carrier_offset + Self::SYMBOL_LENGTH) % Self::SYMBOL_LENGTH
    }

    fn mod_map(b: &[bool; Self::MOD_BITS]) -> Complex32 {
        Psk::<4>::map(b)
    }

    fn base37(str: &[u8]) -> u64 {
        fn base37_map(c: u8) -> u8 {
            if c >= b'0' && c <= b'9' {
                return c - b'0' + 1;
            }
            if c >= b'a' && c <= b'z' {
                return c - b'a' + 11;
            }
            if c >= b'A' && c <= b'Z' {
                return c - b'A' + 11;
            }
            0
        }

        let mut acc = 0u64;
        for c in str {
            acc = 37 * acc + base37_map(*c) as u64;
        }
        acc
    }

    fn noise_symbol(&mut self) {
        let factor = Self::SYMBOL_LENGTH as f32 / Self::PAY_CAR_CNT as f32;
        self.freq.fill(Complex32::new(0.0, 0.0));
        for i in 0..Self::PAY_CAR_CNT {
            self.freq[self.bin(i + Self::PAY_CAR_OFF)] =
                factor * Complex32::new(Self::nrz(self.mls.next()), Self::nrz(self.mls.next()));
        }
        self.transform(false);
    }

    fn payload_symbol(&mut self) {
        self.freq.fill(Complex32::new(0.0, 0.0));

        for i in 0..Self::PAY_CAR_CNT {
            let index = Self::MOD_BITS * (Self::PAY_CAR_CNT * self.symbol_number + i);
            self.prev[i] *= Self::mod_map(&self.code[index..index + 2].try_into().unwrap());
            self.freq[self.bin(i + Self::PAY_CAR_OFF)] = self.prev[i];
        }
        self.transform(true);
    }

    fn silence(&mut self) {
        self.temp.fill(Complex32::new(0.0, 0.0));
    }

    fn transform(&mut self, _papr_reduction: bool) {
        // TODO
        // if papr_reduction && RATE <= 16000 {
        // 	improve_papr(freq);
        //         }
        self.fft
            .process_outofplace_with_scratch(self.freq, &mut self.temp, &mut self.fft_scratch);
        for i in 0..Self::SYMBOL_LENGTH {
            self.temp[i] /= (8 * Self::SYMBOL_LENGTH).sqrt();
        }
    }

    // fn schmidl_cox() {
    // 	CODE::MLS seq(cor_seq_poly);
    // 	float factor = std::sqrt(float(2 * symbol_length) / cor_seq_len);
    // 	for (int i = 0; i < symbol_length; ++i)
    // 		freq[i] = 0;
    // 	freq[bin(cor_seq_off - 2)] = factor;
    // 	for (int i = 0; i < cor_seq_len; ++i)
    // 		freq[bin(2 * i + cor_seq_off)] = nrz(seq());
    // 	for (int i = 0; i < cor_seq_len; ++i)
    // 		freq[bin(2 * i + cor_seq_off)] *= freq[bin(2 * (i - 1) + cor_seq_off)];
    // 	transform(false);
    // }
}

// template<int RATE>
// class Encoder : public EncoderInterface {
// 	DSP::FastFourierTransform<symbol_length, cmplx, 1> bwd;
// 	CODE::CRC<uint16_t> crc;
// 	CODE::BoseChaudhuriHocquenghemEncoder<255, 71> bch;
// 	CODE::MLS noise_seq;
// 	ImprovePAPR<cmplx, symbol_length, RATE <= 16000 ? 4 : 1> improve_papr;
// 	PolarEncoder<code_type> polar;
// 	cmplx temp[extended_length], freq[symbol_length], prev[pay_car_cnt], guard[guard_length];
// 	uint8_t mesg[max_bits / 8], call[9];
// 	code_type code[code_len];
// 	uint64_t meta_data;
// 	int operation_mode = 0;
// 	int carrier_offset = 0;
// 	int symbol_number = symbol_count;
// 	int count_down = 0;
// 	int fancy_line = 0;
// 	int noise_count = 0;
//
//
//
// 	void schmidl_cox() {
// 		CODE::MLS seq(cor_seq_poly);
// 		float factor = std::sqrt(float(2 * symbol_length) / cor_seq_len);
// 		for (int i = 0; i < symbol_length; ++i)
// 			freq[i] = 0;
// 		freq[bin(cor_seq_off - 2)] = factor;
// 		for (int i = 0; i < cor_seq_len; ++i)
// 			freq[bin(2 * i + cor_seq_off)] = nrz(seq());
// 		for (int i = 0; i < cor_seq_len; ++i)
// 			freq[bin(2 * i + cor_seq_off)] *= freq[bin(2 * (i - 1) + cor_seq_off)];
// 		transform(false);
// 	}
//
// 	void preamble() {
// 		uint8_t data[9] = {0}, parity[23] = {0};
// 		for (int i = 0; i < 55; ++i)
// 			CODE::set_be_bit(data, i, (meta_data >> i) & 1);
// 		crc.reset();
// 		uint16_t cs = crc(meta_data << 9);
// 		for (int i = 0; i < 16; ++i)
// 			CODE::set_be_bit(data, i + 55, (cs >> i) & 1);
// 		bch(data, parity);
// 		CODE::MLS seq(pre_seq_poly);
// 		float factor = std::sqrt(float(symbol_length) / pre_seq_len);
// 		for (int i = 0; i < symbol_length; ++i)
// 			freq[i] = 0;
// 		freq[bin(pre_seq_off - 1)] = factor;
// 		for (int i = 0; i < 71; ++i)
// 			freq[bin(i + pre_seq_off)] = nrz(CODE::get_be_bit(data, i));
// 		for (int i = 71; i < pre_seq_len; ++i)
// 			freq[bin(i + pre_seq_off)] = nrz(CODE::get_be_bit(parity, i - 71));
// 		for (int i = 0; i < pre_seq_len; ++i)
// 			freq[bin(i + pre_seq_off)] *= freq[bin(i - 1 + pre_seq_off)];
// 		for (int i = 0; i < pre_seq_len; ++i)
// 			freq[bin(i + pre_seq_off)] *= nrz(seq());
// 		for (int i = 0; i < pay_car_cnt; ++i)
// 			prev[i] = freq[bin(i + pay_car_off)];
// 		transform();
// 	}
//
// 	void fancy_symbol() {
// 		int active_carriers = 1;
// 		for (int j = 0; j < 9; ++j)
// 			for (int i = 0; i < 8; ++i)
// 				active_carriers += (base37_bitmap[call[j] + 37 * fancy_line] >> i) & 1;
// 		float factor = std::sqrt(float(symbol_length) / active_carriers);
// 		for (int i = 0; i < symbol_length; ++i)
// 			freq[i] = 0;
// 		for (int j = 0; j < 9; ++j)
// 			for (int i = 0; i < 8; ++i)
// 				if (base37_bitmap[call[j] + 37 * fancy_line] & (1 << (7 - i)))
// 					freq[bin((8 * j + i) * 3 + fancy_off)] = factor * nrz(noise_seq());
// 		transform(false);
// 	}
//
//
// public:
// 	Encoder() : noise_seq(noise_poly), crc(0xA8F4), bch({
// 		0b100011101, 0b101110111, 0b111110011, 0b101101001,
// 		0b110111101, 0b111100111, 0b100101011, 0b111010111,
// 		0b000010011, 0b101100101, 0b110001011, 0b101100011,
// 		0b100011011, 0b100111111, 0b110001101, 0b100101101,
// 		0b101011111, 0b111111001, 0b111000011, 0b100111001,
// 		0b110101001, 0b000011111, 0b110000111, 0b110110001}) {}
//
// 	int rate() final {
// 		return RATE;
// 	}
//
// 	bool produce(int16_t *audio_buffer, int channel_select) final {
// 		bool data_symbol = false;
// 		switch (count_down) {
// 			case 5:
// 				if (noise_count) {
// 					--noise_count;
// 					noise_symbol();
// 					break;
// 				}
// 				--count_down;
// 			case 4:
// 				schmidl_cox();
// 				data_symbol = true;
// 				--count_down;
// 				break;
// 			case 3:
// 				preamble();
// 				data_symbol = true;
// 				--count_down;
// 				if (!operation_mode)
// 					--count_down;
// 				break;
// 			case 2:
// 				payload_symbol();
// 				data_symbol = true;
// 				if (++symbol_number == symbol_count)
// 					--count_down;
// 				break;
// 			case 1:
// 				if (fancy_line) {
// 					--fancy_line;
// 					fancy_symbol();
// 					break;
// 				}
// 				silence();
// 				--count_down;
// 				break;
// 			default:
// 				for (int i = 0; i < extended_length; ++i)
// 					next_sample(audio_buffer, 0, channel_select, i);
// 				return false;
// 		}
// 		for (int i = 0; i < guard_length; ++i) {
// 			float x = i / float(guard_length - 1);
// 			float ratio(0.5);
// 			if (data_symbol)
// 				x = std::min(x, ratio) / ratio;
// 			float y = 0.5f * (1 - std::cos(DSP::Const<float>::Pi() * x));
// 			cmplx sum = DSP::lerp(guard[i], temp[i + symbol_length - guard_length], y);
// 			next_sample(audio_buffer, sum, channel_select, i);
// 		}
// 		for (int i = 0; i < guard_length; ++i)
// 			guard[i] = temp[i];
// 		for (int i = 0; i < symbol_length; ++i)
// 			next_sample(audio_buffer, temp[i], channel_select, i + guard_length);
// 		return true;
// 	}
//
// 	void configure(const uint8_t *payload, const int8_t *call_sign, int carrier_frequency, int noise_symbols, bool fancy_header) final {
// 		int len = 0;
// 		while (len <= 128 && payload[len])
// 			++len;
// 		if (!len)
// 			operation_mode = 0;
// 		else if (len <= 85)
// 			operation_mode = 16;
// 		else if (len <= 128)
// 			operation_mode = 15;
// 		else
// 			operation_mode = 14;
// 		carrier_offset = (carrier_frequency * symbol_length) / RATE;
// 		meta_data = (base37(call_sign) << 8) | operation_mode;
// 		for (int i = 0; i < 9; ++i)
// 			call[i] = 0;
// 		for (int i = 0; i < 9 && call_sign[i]; ++i)
// 			call[i] = base37_map(call_sign[i]);
// 		symbol_number = 0;
// 		count_down = 5;
// 		fancy_line = 11 * fancy_header;
// 		noise_count = noise_symbols;
// 		for (int i = 0; i < guard_length; ++i)
// 			guard[i] = 0;
// 		const uint32_t *frozen_bits;
// 		int data_bits;
// 		switch (operation_mode) {
// 			case 14:
// 				data_bits = 1360;
// 				frozen_bits = frozen_2048_1392;
// 				break;
// 			case 15:
// 				data_bits = 1024;
// 				frozen_bits = frozen_2048_1056;
// 				break;
// 			case 16:
// 				data_bits = 680;
// 				frozen_bits = frozen_2048_712;
// 				break;
// 			default:
// 				return;
// 		}
// 		CODE::Xorshift32 scrambler;
// 		for (int i = 0; i < data_bits / 8; ++i)
// 			mesg[i] = payload[i] ^ scrambler();
// 		polar(code, mesg, frozen_bits, data_bits);
// 	}
// };
