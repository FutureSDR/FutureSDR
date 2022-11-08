use bimap::BiMap;
use std::fmt;

mod tx_audio;
pub use crate::tx_audio::run_fg;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum CWAlphabet {
    Dot,
    Dash,
    LetterSpace,
    WordSpace,
    Unknown,
}

impl fmt::Display for CWAlphabet {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CWAlphabet::Dot => write!(f, "."),
            CWAlphabet::Dash => write!(f, "-"),
            CWAlphabet::LetterSpace => write!(f, " "),
            CWAlphabet::WordSpace => write!(f, "/ "),
            CWAlphabet::Unknown => write!(f, " <?> "),
        }
    }
}

pub fn get_alphabet() -> BiMap<char, Vec<CWAlphabet>> {
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
    alphabet.insert('.', vec![Dot, Dash, Dot, Dash, Dot, Dash]);
    alphabet.insert(',', vec![Dash, Dash, Dot, Dot, Dash, Dash]);
    alphabet.insert('?', vec![Dot, Dot, Dash, Dash, Dot, Dot]);
    alphabet.insert(';', vec![Dash, Dot, Dash, Dot, Dash, Dot]);
    alphabet.insert(':', vec![Dash, Dash, Dash, Dot, Dot, Dot]);
    alphabet.insert('-', vec![Dash, Dot, Dot, Dot, Dot, Dash]);
    alphabet.insert('/', vec![Dash, Dot, Dot, Dash, Dot]);
    alphabet.insert('"', vec![Dot, Dash, Dot, Dot, Dash, Dot]);
    alphabet.insert('\'', vec![Dot, Dash, Dash, Dash, Dot]);
    alphabet.insert(' ', vec![WordSpace]);

    alphabet
}

pub fn char_to_bb(dot_len: usize) -> impl FnMut(&char) -> Vec<f32> {
    use CWAlphabet::*;
    let alphabet = get_alphabet();

    move |c: &char| {
        let v = alphabet
            .get_by_left(c)
            .cloned()
            .unwrap_or_else(|| vec![Unknown; 1]);
        v.into_iter()
            .flat_map(|x| match x {
                Dot => [vec![1.0; dot_len], vec![0.0; dot_len]].concat(),
                Dash => [vec![1.0; 3 * dot_len], vec![0.0; dot_len]].concat(),
                LetterSpace => panic!("LetterSpace shouldn't occur in char."),
                Unknown => vec![0.0; 3 * dot_len],
                WordSpace => vec![0.0; 5 * dot_len], // other 3 spaces are chained
            })
            .chain(vec![0.0; 2 * dot_len])
            .collect()
    }
}

pub fn msg_to_cw(msg: &[char]) -> Vec<CWAlphabet> {
    let alphabet = get_alphabet();

    msg.iter()
        .flat_map(|x| match alphabet.get_by_left(x) {
            Some(v) => {
                if v[0] == CWAlphabet::WordSpace {
                    vec![CWAlphabet::WordSpace; 1]
                } else {
                    [v.clone(), vec![CWAlphabet::LetterSpace; 1]].concat()
                }
            }
            None => vec![CWAlphabet::Unknown; 1],
        })
        .collect()
}
