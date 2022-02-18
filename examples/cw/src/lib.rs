use futuresdr::anyhow::Result;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::audio::Oscillator;
use futuresdr::blocks::ApplyIntoIter;
use futuresdr::blocks::Combine;
#[cfg(not(target_arch = "wasm32"))]
use futuresdr::blocks::ConsoleSink;
use futuresdr::blocks::VectorSourceBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use std::fmt;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[derive(Copy, Clone)]
pub enum CWAlphabet {
    Dot,
    Dash,
    LetterSpace,
    WordSpace,
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

#[rustfmt::skip]
fn morse(i: &char) -> Vec<CWAlphabet> {
    match i {
        'A' => vec![CWAlphabet::Dot, CWAlphabet::Dash, CWAlphabet::LetterSpace],
        'B' => vec![CWAlphabet::Dash, CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::LetterSpace],
        'C' => vec![CWAlphabet::Dash, CWAlphabet::Dot, CWAlphabet::Dash, CWAlphabet::Dot, CWAlphabet::LetterSpace],
        'D' => vec![CWAlphabet::Dash, CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::LetterSpace],
        'E' => vec![CWAlphabet::Dot, CWAlphabet::LetterSpace],
        'F' => vec![CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::Dash, CWAlphabet::Dot, CWAlphabet::LetterSpace],
        'G' => vec![CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::Dot, CWAlphabet::LetterSpace],
        'H' => vec![CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::LetterSpace],
        'I' => vec![CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::LetterSpace],
        'J' => vec![CWAlphabet::Dot, CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::LetterSpace],
        'K' => vec![CWAlphabet::Dash, CWAlphabet::Dot, CWAlphabet::Dash, CWAlphabet::LetterSpace],
        'L' => vec![CWAlphabet::Dot, CWAlphabet::Dash, CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::LetterSpace],
        'M' => vec![CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::LetterSpace],
        'N' => vec![CWAlphabet::Dash, CWAlphabet::Dot, CWAlphabet::LetterSpace],
        'O' => vec![CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::LetterSpace],
        'P' => vec![CWAlphabet::Dot, CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::Dot, CWAlphabet::LetterSpace],
        'Q' => vec![CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::Dot, CWAlphabet::Dash, CWAlphabet::LetterSpace],
        'R' => vec![CWAlphabet::Dot, CWAlphabet::Dash, CWAlphabet::Dot, CWAlphabet::LetterSpace],
        'S' => vec![CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::LetterSpace],
        'T' => vec![CWAlphabet::Dash, CWAlphabet::LetterSpace],
        'U' => vec![CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::Dash, CWAlphabet::LetterSpace],
        'V' => vec![CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::Dash, CWAlphabet::LetterSpace],
        'W' => vec![CWAlphabet::Dot, CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::LetterSpace],
        'X' => vec![CWAlphabet::Dash, CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::Dash, CWAlphabet::LetterSpace],
        'Y' => vec![CWAlphabet::Dash, CWAlphabet::Dot, CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::LetterSpace],
        'Z' => vec![CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::LetterSpace],
        '0' => vec![CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::LetterSpace],
        '1' => vec![CWAlphabet::Dot, CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::LetterSpace],
        '2' => vec![CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::LetterSpace],
        '3' => vec![CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::LetterSpace],
        '4' => vec![CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::Dash, CWAlphabet::LetterSpace],
        '5' => vec![CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::LetterSpace],
        '6' => vec![CWAlphabet::Dash, CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::LetterSpace],
        '7' => vec![CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::LetterSpace],
        '8' => vec![CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::Dot, CWAlphabet::Dot, CWAlphabet::LetterSpace],
        '9' => vec![CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::Dash, CWAlphabet::Dot, CWAlphabet::LetterSpace],
        _ => vec![CWAlphabet::WordSpace],
    }
}

const SAMPLE_RATE: usize = 48_000;
const SIDETONE_FREQ: f32 = 440.0; // Usually between 400Hz and 750Hz
const DOT_LENGTH: usize = SAMPLE_RATE / 20;

impl IntoIterator for CWAlphabet {
    type Item = f32;
    type IntoIter = std::iter::Chain<
        std::iter::Take<std::iter::Repeat<f32>>,
        std::iter::Take<std::iter::Repeat<f32>>,
    >;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            CWAlphabet::Dot => std::iter::repeat(1.0)
                .take(DOT_LENGTH)
                .chain(std::iter::repeat(0.0).take(DOT_LENGTH)),
            CWAlphabet::Dash => std::iter::repeat(1.0)
                .take(3 * DOT_LENGTH)
                .chain(std::iter::repeat(0.0).take(DOT_LENGTH)),
            CWAlphabet::LetterSpace => std::iter::repeat(0.0)
                .take(3 * DOT_LENGTH)
                .chain(std::iter::repeat(0.0).take(0)),
            CWAlphabet::WordSpace => std::iter::repeat(0.0)
                .take((5 - 2) * DOT_LENGTH)
                .chain(std::iter::repeat(0.0).take(0)),
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn run_fg(msg: String) {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    run_fg_impl(msg).await.unwrap();
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn run_fg(msg: String) -> Result<()> {
    run_fg_impl(msg).await
}

pub async fn run_fg_impl(msg: String) -> Result<()> {
    let msg: Vec<char> = msg.to_uppercase().chars().collect();

    let mut fg = Flowgraph::new();
    let src = fg.add_block(VectorSourceBuilder::<char>::new(msg).build());
    let audio_snk = fg.add_block(AudioSink::new(SAMPLE_RATE.try_into().unwrap(), 1));
    let morse = fg.add_block(ApplyIntoIter::<char, Vec<CWAlphabet>>::new(&morse));
    let switch_command = fg.add_block(ApplyIntoIter::<CWAlphabet, CWAlphabet>::new(
        |c: &CWAlphabet| *c,
    ));
    let sidetone_src = fg.add_block(Oscillator::new(SIDETONE_FREQ, 0.2));
    let switch_sidetone = fg.add_block(Combine::new(|a: &f32, b: &f32| -> f32 { *a * *b }));

    fg.connect_stream(src, "out", morse, "in")?;
    fg.connect_stream(morse, "out", switch_command, "in")?;
    fg.connect_stream(switch_command, "out", switch_sidetone, "in0")?;
    fg.connect_stream(sidetone_src, "out", switch_sidetone, "in1")?;
    fg.connect_stream(switch_sidetone, "out", audio_snk, "in")?;

    #[cfg(not(target_arch = "wasm32"))]
    {
        let console = fg.add_block(ConsoleSink::<CWAlphabet>::new(""));
        fg.connect_stream(morse, "out", console, "in")?;
    }

    Runtime::new().run_async(fg).await?;
    Ok(())
}
