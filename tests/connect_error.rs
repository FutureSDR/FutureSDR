use futuresdr::anyhow::Result;
use futuresdr::blocks::Fft;
use futuresdr::blocks::MessageSink;
use futuresdr::blocks::MessageSource;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::runtime::Error;
use futuresdr::runtime::Flowgraph;
use futuresdr_types::Pmt;
use num_complex::Complex;
use std::time::Duration;

#[test]
fn connect_type_error() -> Result<()> {
    let mut fg = Flowgraph::new();
    let fft = Fft::new(1024);
    let sink = NullSink::<[Complex<f32>; 1024]>::new();

    let fft = fg.add_block(fft)?;
    let sink = fg.add_block(sink)?;

    let result = fg.connect_stream(fft, "out", sink, "in");
    assert!(result.is_err());

    let error = result.unwrap_err();
    match error {
        o @ Error::ConnectError { .. } => {
            let msg = o.to_string();
            // Token test for type info.
            assert!(msg.contains("num_complex::Complex<f32>"));
        }
        _ => panic!("Expected ConnectError"),
    };
    Ok(())
}

#[test]
fn message_invalid_in_port() -> Result<()> {
    let mut fg = Flowgraph::new();
    let source = MessageSource::new(Pmt::Ok, Duration::from_secs(1), Some(1));
    let source = fg.add_block(source)?;
    let sink = MessageSink::new();
    let sink = fg.add_block(sink)?;

    let result = fg.connect_message(source, "out", sink, "non_existent");
    assert!(result.is_err());

    let error = result.unwrap_err();
    match error {
        o @ Error::InvalidMessagePort { .. } => {
            let msg = o.to_string();
            assert!(
                msg.contains("MessageSink"),
                "\"{msg}\" does not contain 'MessageSink'"
            );
        }
        _ => panic!("Expected ConnectError"),
    };
    Ok(())
}

#[test]
fn message_invalid_out_port() -> Result<()> {
    let mut fg = Flowgraph::new();
    let source = MessageSource::new(Pmt::Ok, Duration::from_secs(1), Some(1));
    let source = fg.add_block(source)?;
    let sink = MessageSink::new();
    let sink = fg.add_block(sink)?;

    let result = fg.connect_message(source, "fictitious", sink, "in");
    assert!(result.is_err());

    let error = result.unwrap_err();
    match error {
        o @ Error::InvalidMessagePort { .. } => {
            let msg = o.to_string();
            assert!(
                msg.contains("MessageSource"),
                "\"{msg}\" does not contain 'MessageSource'"
            );
        }
        _ => panic!("Expected InvalidMessagePort"),
    };
    Ok(())
}

#[test]
fn stream_invalid_in_port() -> Result<()> {
    let mut fg = Flowgraph::new();
    let source = NullSource::<f32>::new();
    let source = fg.add_block(source)?;
    let sink = NullSink::<f32>::new();
    let sink = fg.add_block(sink)?;

    let result = fg.connect_stream(source, "out", sink, "non_existent");
    assert!(result.is_err());

    let error = result.unwrap_err();
    match error {
        o @ Error::InvalidStreamPort { .. } => {
            let msg = o.to_string();
            assert!(
                msg.contains("NullSink"),
                "\"{msg}\" does not contain 'NullSink'"
            );
        }
        _ => panic!("Expected InvalidStreamPort"),
    };
    Ok(())
}

#[test]
fn stream_invalid_out_port() -> Result<()> {
    let mut fg = Flowgraph::new();
    let source = NullSource::<f32>::new();
    let source = fg.add_block(source)?;
    let sink = NullSink::<f32>::new();
    let sink = fg.add_block(sink)?;

    let result = fg.connect_stream(source, "fictitious", sink, "in");
    assert!(result.is_err());

    let error = result.unwrap_err();
    match error {
        o @ Error::InvalidStreamPort { .. } => {
            let msg = o.to_string();
            assert!(
                msg.contains("NullSource"),
                "\"{msg}\" does not contain 'NullSource'"
            );
        }
        _ => panic!("Expected InvalidStreamPort"),
    };
    Ok(())
}
