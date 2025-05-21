use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::ApplyIntoIter;
use futuresdr::blocks::Combine;
use futuresdr::blocks::SignalSourceBuilder;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;
use std::fmt;

#[derive(Copy, Clone)]
pub enum CWAlphabet {
    Dot,
    Dash,
    LetterSpace,
    WordSpace,
}

impl Default for CWAlphabet {
    fn default() -> Self {
        Self::Dot
    }
}

impl fmt::Debug for CWAlphabet {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            CWAlphabet::Dot => write!(f, "."),
            CWAlphabet::Dash => write!(f, "-"),
            CWAlphabet::LetterSpace => write!(f, " "),
            CWAlphabet::WordSpace => write!(f, " / "),
        }
    }
}

fn morse(i: &char) -> Vec<CWAlphabet> {
    use CWAlphabet::*;
    match i {
        'A' => vec![Dot, Dash, LetterSpace],
        'B' => vec![Dash, Dot, Dot, Dot, LetterSpace],
        'C' => vec![Dash, Dot, Dash, Dot, LetterSpace],
        'D' => vec![Dash, Dot, Dot, LetterSpace],
        'E' => vec![Dot, LetterSpace],
        'F' => vec![Dot, Dot, Dash, Dot, LetterSpace],
        'G' => vec![Dash, Dash, Dot, LetterSpace],
        'H' => vec![Dot, Dot, Dot, Dot, LetterSpace],
        'I' => vec![Dot, Dot, LetterSpace],
        'J' => vec![Dot, Dash, Dash, Dash, LetterSpace],
        'K' => vec![Dash, Dot, Dash, LetterSpace],
        'L' => vec![Dot, Dash, Dot, Dot, LetterSpace],
        'M' => vec![Dash, Dash, LetterSpace],
        'N' => vec![Dash, Dot, LetterSpace],
        'O' => vec![Dash, Dash, Dash, LetterSpace],
        'P' => vec![Dot, Dash, Dash, Dot, LetterSpace],
        'Q' => vec![Dash, Dash, Dot, Dash, LetterSpace],
        'R' => vec![Dot, Dash, Dot, LetterSpace],
        'S' => vec![Dot, Dot, Dot, LetterSpace],
        'T' => vec![Dash, LetterSpace],
        'U' => vec![Dot, Dot, Dash, LetterSpace],
        'V' => vec![Dot, Dot, Dot, Dash, LetterSpace],
        'W' => vec![Dot, Dash, Dash, LetterSpace],
        'X' => vec![Dash, Dot, Dot, Dash, LetterSpace],
        'Y' => vec![Dash, Dot, Dash, Dash, LetterSpace],
        'Z' => vec![Dash, Dash, Dot, Dot, LetterSpace],
        '0' => vec![Dash, Dash, Dash, Dash, Dash, LetterSpace],
        '1' => vec![Dot, Dash, Dash, Dash, Dash, LetterSpace],
        '2' => vec![Dot, Dot, Dash, Dash, Dash, LetterSpace],
        '3' => vec![Dot, Dot, Dot, Dash, Dash, LetterSpace],
        '4' => vec![Dot, Dot, Dot, Dot, Dash, LetterSpace],
        '5' => vec![Dot, Dot, Dot, Dot, Dot, LetterSpace],
        '6' => vec![Dash, Dot, Dot, Dot, Dot, LetterSpace],
        '7' => vec![Dash, Dash, Dot, Dot, Dot, LetterSpace],
        '8' => vec![Dash, Dash, Dash, Dot, Dot, LetterSpace],
        '9' => vec![Dash, Dash, Dash, Dash, Dot, LetterSpace],
        '.' => vec![Dot, Dash, Dot, Dash, Dot, Dash, LetterSpace],
        ',' => vec![Dash, Dash, Dot, Dot, Dash, Dash, LetterSpace],
        '?' => vec![Dot, Dot, Dash, Dash, Dot, Dot, LetterSpace],
        ';' => vec![Dash, Dot, Dash, Dot, Dash, Dot, LetterSpace],
        ':' => vec![Dash, Dash, Dash, Dot, Dot, Dot, LetterSpace],
        '-' => vec![Dash, Dot, Dot, Dot, Dot, Dash, LetterSpace],
        '/' => vec![Dash, Dot, Dot, Dash, Dot, LetterSpace],
        '"' => vec![Dot, Dash, Dot, Dot, Dash, Dot, LetterSpace],
        '\'' => vec![Dot, Dash, Dash, Dash, Dot, LetterSpace],
        _ => vec![WordSpace],
    }
}

const SAMPLE_RATE: usize = 48_000;
const SIDETONE_FREQ: f32 = 700.0; // Usually between 400Hz and 750Hz
const DOT_LENGTH: usize = SAMPLE_RATE / 20;

impl IntoIterator for CWAlphabet {
    type Item = f32;
    type IntoIter = std::iter::Chain<std::iter::RepeatN<f32>, std::iter::RepeatN<f32>>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            CWAlphabet::Dot => {
                std::iter::repeat_n(1.0, DOT_LENGTH).chain(std::iter::repeat_n(0.0, DOT_LENGTH))
            }
            CWAlphabet::Dash => {
                std::iter::repeat_n(1.0, 3 * DOT_LENGTH).chain(std::iter::repeat_n(0.0, DOT_LENGTH))
            }
            CWAlphabet::LetterSpace => {
                std::iter::repeat_n(0.0, 2 * DOT_LENGTH).chain(std::iter::repeat_n(0.0, 0))
            }
            CWAlphabet::WordSpace => {
                std::iter::repeat_n(0.0, 4 * DOT_LENGTH).chain(std::iter::repeat_n(0.0, 0))
            }
        }
    }
}

pub async fn run_fg(msg: String) -> Result<()> {
    let msg: Vec<char> = msg.to_uppercase().chars().collect();

    let mut fg = Flowgraph::new();
    let src = VectorSource::<char>::new(msg);
    let audio_snk: AudioSink = AudioSink::new(SAMPLE_RATE.try_into().unwrap(), 1);
    let morse = ApplyIntoIter::<_, _, Vec<CWAlphabet>>::new(morse);
    let switch_command = ApplyIntoIter::<_, _, CWAlphabet>::new(|c: &CWAlphabet| *c);
    let sidetone_src = SignalSourceBuilder::<f32>::sin(SIDETONE_FREQ, SAMPLE_RATE as f32, 0.5, 0.0);
    let switch_sidetone: Combine<_, f32, f32, f32> =
        Combine::new(|a: &f32, b: &f32| -> f32 { *a * *b });

    connect!(fg, src > morse > switch_command > in0.switch_sidetone;
        sidetone_src > in1.switch_sidetone.output > audio_snk);

    Runtime::new().run_async(fg).await?;
    Ok(())
}
