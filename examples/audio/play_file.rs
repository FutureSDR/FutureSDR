use futuresdr::anyhow::Result;
use futuresdr::blocks::ApplyIntoIter;
use futuresdr::blocks::ResettableIteratorBlock;
use futuresdr::blocks::Resettable;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::audio::FileSource;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

#[derive(Copy, Clone)]
struct LinearResamplerIterator {
    interpolation: u32,
    decimation: usize,
    increment: f32,
    i: u32,
    decimation_counter: usize,
    previous_input: f32,
    current_input: f32,
}

impl LinearResamplerIterator {
    pub fn new(
        interpolation: usize,
        decimation: usize,
    ) -> Self {
        Self {
            decimation: decimation,
            interpolation: interpolation as u32,
            increment: 0.0,
            i: 0,
            decimation_counter: 0,
            previous_input: 0.0,
            current_input: 0.0,
        }
    }
}

impl Resettable for LinearResamplerIterator {
    type Input = f32;

    fn reset_for(&mut self, new_current: &f32) {
        self.i = 0;
        self.increment = (*new_current - self.current_input)/ (self.interpolation as f32);
        self.previous_input = self.current_input;
        self.current_input = *new_current;
    }
}

impl Iterator for LinearResamplerIterator {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        while self.i < self.interpolation {
            let current_output = self.previous_input + (self.i as f32) * self.increment;
            self.i += 1;
            let current_counter = self.decimation_counter;
            self.decimation_counter = (self.decimation_counter + 1) % self.decimation;
            if current_counter == 0 {
                return Some(current_output);
            }
        }
        return Option::None;
    }
}



fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    // The original mp3 is sampled at 44.1kHz while we force the audio output to be 48kHz
    // thus we upsample by 1:480
    // and downsample by 441:1
    // NB: Obviously some filters should have been used to avoid some artifacts. 
    // Overall it thus converts a 44.1kHz stream into a 48kHz one.
    let interpolation = 480;
    let decimation = 441;

    let src = FileSource::new("rick.mp3");
    let inner = src.kernel::<FileSource>().unwrap();
    let snk = AudioSink::new(inner.sample_rate(), inner.channels());

    let src = fg.add_block(src);
    let snk = fg.add_block(snk);
    let duplicator = fg.add_block(duplicator);

    fg.connect_stream(src, "out", resampler, "in")?;
    fg.connect_stream(resampler, "out", duplicator, "in")?;
    fg.connect_stream(duplicator, "out", snk, "in")?;

    Runtime::new().run(fg)?;

    Ok(())
}
