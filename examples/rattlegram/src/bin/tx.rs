use rattlegram::Encoder;
use std::fs::File;
use std::io::Write;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut e = Encoder::new();
    let payload = b"ja lol ey";
    let call_sign = b"DF1BBL";
    let carrier_frequency = 2000;
    let noise_symbols = 5;
    let fancy_header = false;

    let sig = e.encode(
        payload,
        call_sign,
        carrier_frequency,
        noise_symbols,
        fancy_header,
    );

    println!("{} samples", sig.len());

    let mut out_file = File::create(Path::new("output.f32"))?;
    for s in &sig {
        out_file.write_all(&s.to_le_bytes()).expect("write failed");
    }

    let mut out_file = File::create(Path::new("output.wav"))?;
    let header = wav::Header::new(wav::header::WAV_FORMAT_IEEE_FLOAT, 1, 48_000, 32);
    wav::write(header, &wav::BitDepth::ThirtyTwoFloat(sig), &mut out_file)?;

    Ok(())
}
