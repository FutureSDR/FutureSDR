mod char_to_cw;

pub use char_to_cw::{CharToCW, CharToCWBuilder};

mod cw_to_char;

pub use cw_to_char::{CWToChar, CWToCharBuilder};

mod cw_to_iq;

pub use cw_to_iq::{CWToIQ, CWToIQBuilder};

mod iq_to_cw;

pub use iq_to_cw::{IQToCW, IQToCWBuilder};

use bimap::BiMap;
use std::fmt::{Display, Result, Formatter};
use core::ops::Range;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum CWAlphabet {
    Dot,
    Dash,
    LetterSpace,
    WordSpace,
}

impl Display for CWAlphabet {
    fn fmt(&self, f: &mut Formatter) -> Result {
        match self {
            CWAlphabet::Dot => write!(f, "."),
            CWAlphabet::Dash => write!(f, "-"),
            CWAlphabet::LetterSpace => write!(f, " "),
            CWAlphabet::WordSpace => write!(f, "/ "),
        }
    }
}

pub struct SmoothSymbol {
    symbol: CWAlphabet,
    samples_per_dot: usize,
    counter: usize,
    num_samples_to_generate: usize,
    ramp_up_range: Range<usize>,
    ramp_down_range: Range<usize>,
    max_power_range: Range<usize>,
    min_power_range: Range<usize>,
    step: f32,
    value: f32,
}

impl SmoothSymbol {
    pub fn new(symbol: CWAlphabet,
           transition_smoothness: f32,
           samples_per_dot: usize,
    ) -> Self {
        let factor = match symbol {
            CWAlphabet::Dot => 1,
            CWAlphabet::Dash => 3,
            CWAlphabet::LetterSpace => 2,
            CWAlphabet::WordSpace => 2,
        };

        let num_smooth_samples = (samples_per_dot as f32 * (transition_smoothness / 100.)) as usize;
        let step = 1. / num_smooth_samples as f32;
        let num_samples_to_generate = factor * samples_per_dot;

        SmoothSymbol {
            symbol,
            samples_per_dot,
            counter: 0,
            num_samples_to_generate: num_samples_to_generate,
            ramp_up_range: 0..num_smooth_samples,
            ramp_down_range: num_samples_to_generate - num_smooth_samples..num_samples_to_generate,
            max_power_range: num_smooth_samples..num_samples_to_generate - num_smooth_samples,
            min_power_range: num_samples_to_generate..num_samples_to_generate + samples_per_dot,
            step,
            value: 0.,
        }
    }
}

impl Iterator for SmoothSymbol {
    // We can refer to this type using Self::Item
    type Item = f32;

    // Here, we define the sequence using `.curr` and `.next`.
    // The return type is `Option<T>`:
    //     * When the `Iterator` is finished, `None` is returned.
    //     * Otherwise, the next value is wrapped in `Some` and returned.
    // We use Self::Item in the return type, so we can change
    // the type without having to update the function signatures.
    fn next(&mut self) -> Option<Self::Item> {
        let ret;

        if self.symbol == CWAlphabet::Dot || self.symbol == CWAlphabet::Dash {
            ret = match self.counter {
                x if (&self.ramp_up_range).contains(&x) => { self.value += self.step; Some(self.value) },
                x if (&self.ramp_down_range).contains(&x) => { self.value -= self.step; Some(self.value) },
                x if (&self.max_power_range).contains(&x) => { Some(1.) },
                x if (&self.min_power_range).contains(&x) => { Some(0.) },
                _ => { None }
            };
            //ret = r;
        } else { // WordSpace or LetterSpace
            ret = match self.counter {
                x if (0..self.num_samples_to_generate + self.samples_per_dot).contains(&x) => { Some(0.) },
                _ => { None }
            };
            //ret = r;
        }

        self.counter += 1;
        return ret;
    }
}

/*fn get_smooth_iterator(symbol: &CWAlphabet, samples_per_dot: usize, transition_smoothness: f32) -> Chain<FromFn<f32>, Take<Repeat<f32>>> {
    use crate::blocks::cw::CWAlphabet::*;

    let num_smooth_samples = (samples_per_dot as f32 * (transition_smoothness / 100.)) as usize;
    let step = 1./num_smooth_samples as f32;
    let mut count = 0;
    let mut value = 0.;

    let x = match symbol {
        Dot => 1,
        Dash => 3,
        LetterSpace => 2,
        WordSpace => 2,
    };

    let smooth_generator = std::iter::from_fn(move || {
        if count < num_smooth_samples {
            value += step;
        } else if count >= (x * samples_per_dot - num_smooth_samples) {
            value -= step;
        } else {
            value = 1.;
        }
        // Increment our count. This is why we started at zero.
        count += 1;

        // Check to see if we've finished counting or not.
        if count < x * samples_per_dot {
            Some(value)
        } else {
            None
        }
    });

    let zero_generator = std::iter::from_fn(move || {
        count += 1;

        if count < x * samples_per_dot {
            Some(0.)
        } else {
            None
        }
    });

    match symbol {
        Dot => smooth_generator.chain(std::iter::repeat(0.0).take(samples_per_dot)),
        Dash => smooth_generator.chain(std::iter::repeat(0.0).take(samples_per_dot)),
        LetterSpace => zero_generator.chain(std::iter::repeat(0.0).take(0)),
        WordSpace => zero_generator.chain(std::iter::repeat(0.0).take(0)),
    }
}*/

pub fn get_alphabet() -> BiMap::<char, Vec<CWAlphabet>> {
    use CWAlphabet::*;
    let mut alphabet = BiMap::<char, Vec<CWAlphabet>>::new();

    alphabet.insert('A', vec![Dot, Dash]);
    alphabet.insert('B', vec![Dash, Dot, Dot, Dot]);
    alphabet.insert('C', vec![Dash, Dot, Dash, Dot]);
    alphabet.insert('D', vec![Dash, Dot, Dot]);
    alphabet.insert('E', vec![Dot]);
    alphabet.insert('F', vec![Dot, Dot, Dash, Dot]);
    alphabet.insert('G', vec![Dash, Dash, Dot]);
    alphabet.insert('H', vec![Dot, Dot, Dot, Dot]);
    alphabet.insert('I', vec![Dot, Dot]);
    alphabet.insert('J', vec![Dot, Dash, Dash, Dash]);
    alphabet.insert('K', vec![Dash, Dot, Dash]);
    alphabet.insert('L', vec![Dot, Dash, Dot, Dot]);
    alphabet.insert('M', vec![Dash, Dash]);
    alphabet.insert('N', vec![Dash, Dot]);
    alphabet.insert('O', vec![Dash, Dash, Dash]);
    alphabet.insert('P', vec![Dot, Dash, Dash, Dot]);
    alphabet.insert('Q', vec![Dash, Dash, Dot, Dash]);
    alphabet.insert('R', vec![Dot, Dash, Dot]);
    alphabet.insert('S', vec![Dot, Dot, Dot]);
    alphabet.insert('T', vec![Dash]);
    alphabet.insert('U', vec![Dot, Dot, Dash]);
    alphabet.insert('V', vec![Dot, Dot, Dot, Dash]);
    alphabet.insert('W', vec![Dot, Dash, Dash]);
    alphabet.insert('X', vec![Dash, Dot, Dot, Dash]);
    alphabet.insert('Y', vec![Dash, Dot, Dash, Dash]);
    alphabet.insert('Z', vec![Dash, Dash, Dot, Dot]);
    alphabet.insert('0', vec![Dash, Dash, Dash, Dash, Dash]);
    alphabet.insert('1', vec![Dot, Dash, Dash, Dash, Dash]);
    alphabet.insert('2', vec![Dot, Dot, Dash, Dash, Dash]);
    alphabet.insert('3', vec![Dot, Dot, Dot, Dash, Dash]);
    alphabet.insert('4', vec![Dot, Dot, Dot, Dot, Dash]);
    alphabet.insert('5', vec![Dot, Dot, Dot, Dot, Dot]);
    alphabet.insert('6', vec![Dash, Dot, Dot, Dot, Dot]);
    alphabet.insert('7', vec![Dash, Dash, Dot, Dot, Dot]);
    alphabet.insert('8', vec![Dash, Dash, Dash, Dot, Dot]);
    alphabet.insert('9', vec![Dash, Dash, Dash, Dash, Dot]);
    alphabet.insert(' ', vec![WordSpace]);

    alphabet
}