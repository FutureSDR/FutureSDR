use futuresdr::anyhow::Result;
use futuresdr::blocks::ApplyIntoIter;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::audio::FileSource;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use futuresdr::blocks::Filter;

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
    let _inner = src.as_async::<FileSource>().unwrap();


    // Linear interpolation
    let mut previous = Option::None::<f32>;
    let upsampler = fg.add_block(ApplyIntoIter::new(
        move |current: &f32| -> Vec<f32> {
            let mut vec = Vec::<f32>::with_capacity(interpolation);
            if let Some(previous) = previous {
                for i in 0..interpolation {
                    vec.push(previous + (i as f32) * (current - previous)/ (interpolation as f32))
                }
            }
            previous = Some(*current);
            vec
        },
    ));

    // Keep one out of <decimation> samples.
    let mut counter: usize = 0;
    let downsampler = fg.add_block(Filter::new(move |i: &u32| -> Option<u32> {
        let result = if counter == 0 {
            Some(*i)
        } else {
            None
        };
        counter = (counter + 1) % decimation;
        result
    }));

    // Force the output to be 48kHz and stereo.
    let snk = AudioSink::new(48000, 2);

    // Convert a mono stream into a stereo stream.
    // Yet be aware of https://github.com/FutureSDR/FutureSDR/issues/49
    // And also because we want to experience the stereo, we balance sound level
    // from right to left every 3 seconds by applying a sinusoidal coefficient.
    let mut sample_time: u64 = 0;
    const FACTOR: f32 = 2.0 * 3.14159 / ( 3.0 * 44100.0);
    let duplicator = ApplyIntoIter::<f32, Vec<f32>>::new(
        move |s: &f32| -> Vec<f32> {
            let mut vec = Vec::with_capacity(2);
            let coeff = ((sample_time as f32) * FACTOR).sin();
            sample_time += 1;
            vec.push(*s * coeff);
            vec.push(*s * (1.0-coeff));
            return vec;
        }
    );

    let src = fg.add_block(src);
    let snk = fg.add_block(snk);
    let duplicator = fg.add_block(duplicator);

    fg.connect_stream(src, "out", upsampler, "in")?;
    fg.connect_stream(upsampler, "out", downsampler, "in")?;
    fg.connect_stream(downsampler, "out", duplicator, "in")?;
    fg.connect_stream(duplicator, "out", snk, "in")?;

    Runtime::new().run(fg)?;

    Ok(())
}
