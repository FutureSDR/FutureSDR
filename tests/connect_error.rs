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
    let fft: BlockId = fg.add(Fft::new(16) as Fft)?.into();
    let sink: BlockId = fg.add(NullSink::<[Complex<f32>; 16]>::new())?.into();
    let result = fg.connect_dyn(
        fft.dyn_stream_output("output")?,
        sink.dyn_stream_input("input")?,
    );

    match result {
        Err(Error::ValidationError(_)) => Ok(()),
        e => panic!("Expected ValidationError got {e:?}"),
    }
}

#[test]
fn message_invalid_in_port() -> Result<()> {
    let mut fg = Flowgraph::new();
    let source = MessageSource::new(Pmt::Ok, Duration::from_secs(1), Some(1));
    let source = fg.add(source)?;
    let sink = MessageSink::new();
    let sink = fg.add(sink)?;

    let result = fg.connect_message(
        source.dyn_message_output("out")?,
        sink.dyn_message_input("non_existent")?,
    );
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
    let source = fg.add(source)?;
    let sink = MessageSink::new();
    let sink = fg.add(sink)?;

    let result = fg.connect_message(
        source.dyn_message_output("fictitious")?,
        sink.dyn_message_input("in")?,
    );
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
    let source = fg.add(source)?;
    let sink = NullSink::<f32>::new();
    let sink = fg.add(sink)?;

    let result = fg.connect_dyn(
        source.dyn_stream_output("output")?,
        sink.dyn_stream_input("non_existent")?,
    );
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
    let source = fg.add(source)?;
    let sink = NullSink::<f32>::new();
    let sink = fg.add(sink)?;

    let result = fg.connect_dyn(
        source.dyn_stream_output("fictitious")?,
        sink.dyn_stream_input("input")?,
    );
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
