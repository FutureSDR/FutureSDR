use futuresdr::blocks::{Fft, NullSink};
use futuresdr::runtime::{Error, Flowgraph};
use num_complex::Complex;

#[test]
fn connect_type_error() {
    let mut fg = Flowgraph::new();
    let fft = Fft::new(1024);
    let sink = NullSink::<[Complex<f32>; 1024]>::new();

    let fft = fg.add_block(fft);
    let sink = fg.add_block(sink);

    let result = fg.connect_stream(fft, "out", sink, "in");
    assert!(result.is_err());

    let error = result.unwrap_err();
    match error {
        o @ Error::ConnectError { .. } => {
            let msg = o.to_string();
            // println!("{}", msg);
            // Token test for type info.
            assert!(msg.contains("num_complex::Complex<f32>"));
        }
        _ => panic!("Expected ConnectError"),
    };
}
