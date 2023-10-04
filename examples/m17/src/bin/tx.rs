use codec2::Codec2;
use codec2::Codec2Mode;
use futuresdr::anyhow::Result;
use futuresdr::macros::connect;
use futuresdr::blocks::ApplyNM;
use futuresdr::blocks::FiniteSource;
use futuresdr::blocks::FileSink;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use std::fs::File;
use std::path::Path;

use m17::CallSign;
use m17::LinkSetupFrame;
use m17::EncoderBlock;

fn main() -> Result<()> {

    let mut fg = Flowgraph::new();

    let mut in_file = File::open(Path::new("rick.wav"))?;
    let (header, data) = wav::read(&mut in_file)?;
    assert_eq!(header.channel_count, 1);
    assert_eq!(header.sampling_rate, 8000);
    assert_eq!(header.audio_format, wav::WAV_FORMAT_PCM);
    assert_eq!(header.bits_per_sample, 16);
    let data = data.try_into_sixteen().unwrap();

    let mut i = 0;
    let src = FiniteSource::new(move || {
        if i >= data.len() {
            None
        } else {
            i += 1;
            Some(data[i-1])
        }
    });

    let mut c2 = Codec2::new(Codec2Mode::MODE_3200);
    assert_eq!(c2.samples_per_frame(), 160);
    assert_eq!(c2.bits_per_frame(), 64);

    let codec2 = ApplyNM::<_, _, _, 160, {(64 + 7) / 8}>::new(move |i: &[i16], o: &mut [u8]| {
        c2.encode(o, i);
    });

    let lsf = LinkSetupFrame::new(CallSign::new_id("DF1BBL"), CallSign::new_broadcast());
    let encoder = EncoderBlock::new(lsf);

    let snk = FileSink::<f32>::new("syms.f32");
    connect!(fg, src > codec2 > encoder > snk);
    
    let rt = Runtime::new();
    std::thread::sleep_ms(1000);
    rt.run(fg)?;

    Ok(())
}
