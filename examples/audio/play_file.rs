use futuresdr::anyhow::Result;
use futuresdr::blocks::ApplyIntoIter;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::audio::FileSource;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use futuresdr::blocks::Filter;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let interpolation = 480;
    let decimation = 441;

    let src = FileSource::new("rick.mp3");
    let inner = src.as_async::<FileSource>().unwrap();


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

    //let snk = AudioSink::new(inner.sample_rate(), inner.channels());
    let snk = AudioSink::new(48000 /*44100*/, 2);

    fn tuple(s: &f32) -> Vec<f32> {
        let mut vec = Vec::with_capacity(2);
        vec.push(*s*0.7);
        vec.push(*s * 1.5);
        return vec;
    }
    let duplicator = ApplyIntoIter::<f32, Vec<f32>>::new(tuple);

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
