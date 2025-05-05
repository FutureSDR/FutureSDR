use anyhow::Result;
use futuresdr::blocks::Fft;
use futuresdr::blocks::MessageSink;
use futuresdr::blocks::MessageSource;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::prelude::*;
use std::time::Duration;

#[test]
fn connect_type_error() -> Result<()> {
    let mut fg = Flowgraph::new();
    let fft: BlockId = fg.add_block(Fft::new(1024) as Fft).into();
    let sink: BlockId = fg.add_block(NullSink::<[Complex<f32>; 1024]>::new()).into();
    let result = fg.connect_dyn(fft, "output", sink, "input");

    match result {
        Err(Error::ValidationError(_)) => Ok(()),
        e => panic!("Expected ValidationError got {e:?}"),
    }
}

#[test]
fn message_invalid_in_port() -> Result<()> {
    let mut fg = Flowgraph::new();
    let source = MessageSource::new(Pmt::Ok, Duration::from_secs(1), Some(1));
    let source = fg.add_block(source);
    let sink = MessageSink::new();
    let sink = fg.add_block(sink);

    let result = fg.connect_message(&source, "out", &sink, "non_existent");
    assert!(result.is_err());

    let error = result.unwrap_err();
    match error {
        o @ Error::InvalidMessagePort { .. } => {
            let msg = o.to_string();
            assert!(
                msg.contains("non_existent"),
                "\"{msg}\" does not contain 'non_existent'"
            );
        }
        _ => panic!("Expected ConnectError."),
    };
    Ok(())
}

#[test]
fn message_invalid_out_port() -> Result<()> {
    let mut fg = Flowgraph::new();
    let source = MessageSource::new(Pmt::Ok, Duration::from_secs(1), Some(1));
    let source = fg.add_block(source);
    let sink = MessageSink::new();
    let sink = fg.add_block(sink);

    let result = fg.connect_message(&source, "fictitious", &sink, "in");
    assert!(result.is_err());

    let error = result.unwrap_err();
    match error {
        o @ Error::InvalidMessagePort { .. } => {
            let msg = o.to_string();
            assert!(
                msg.contains("fictitious"),
                "\"{msg}\" does not contain 'fictitious'"
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
    let source = fg.add_block(source);
    let sink = NullSink::<f32>::new();
    let sink = fg.add_block(sink);

    let result = fg.connect_dyn(source, "output", sink, "non_existent");
    assert!(result.is_err());

    let error = result.unwrap_err();
    match error {
        o @ Error::InvalidStreamPort { .. } => {
            let msg = o.to_string();
            assert!(
                msg.contains("non_existent"),
                "\"{msg}\" does not contain 'non_existent'"
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
    let source = fg.add_block(source);
    let sink = NullSink::<f32>::new();
    let sink = fg.add_block(sink);

    let result = fg.connect_dyn(source, "fictitious", sink, "input");
    assert!(result.is_err());

    let error = result.unwrap_err();
    match error {
        o @ Error::InvalidStreamPort { .. } => {
            let msg = o.to_string();
            assert!(
                msg.contains("fictitious"),
                "\"{msg}\" does not contain 'fictitious'"
            );
        }
        _ => panic!("Expected InvalidStreamPort"),
    };
    Ok(())
}
