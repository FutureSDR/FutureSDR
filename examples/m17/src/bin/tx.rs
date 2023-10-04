use futuresdr::anyhow::Result;
use std::fs::File;
use std::io::prelude::*;

use m17::CallSign;
use m17::LinkSetupFrame;
use m17::Encoder;

fn main() -> Result<()> {
    let lsf = LinkSetupFrame::new(CallSign::new_id("DF1BBL"), CallSign::new_broadcast());
    let mut encoder = Encoder::new(lsf);

    let data = [0, 1, 2, 3, 4, 5, 6, 7, 8 , 9, 10, 11, 12, 13, 14, 15];
    let mut syms = Vec::new();
    let s = encoder.encode(&data, false);
    syms.extend_from_slice(s);
    let s = encoder.encode(&[0; 16], false);
    syms.extend_from_slice(s);
    let s = encoder.encode(&[0; 16], true);
    syms.extend_from_slice(s);
    let s = encoder.encode(&[0; 16], true);
    syms.extend_from_slice(s);
    let s = encoder.encode(&[0; 16], true);
    syms.extend_from_slice(s);
    let s = encoder.encode(&[0; 16], true);
    syms.extend_from_slice(s);
    let s = encoder.encode(&[0; 16], true);
    syms.extend_from_slice(s);
    let s = encoder.encode(&[0; 16], true);
    syms.extend_from_slice(s);
    let s = encoder.encode(&[0; 16], true);
    syms.extend_from_slice(s);
    let s = encoder.encode(&[0; 16], true);
    syms.extend_from_slice(s);

    let mut file = File::create("syms.f32")?;
    for s in syms {
        file.write_all(&s.to_ne_bytes())?;
    }
    Ok(())
}
