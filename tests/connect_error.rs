use anyhow::Result;
use futuresdr::blocks::Copy;
use futuresdr::blocks::Fft;
use futuresdr::blocks::MessageSink;
use futuresdr::blocks::MessageSource;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::slab;
use std::time::Duration;

#[test]
fn connect_type_error() -> Result<()> {
    let mut fg = Flowgraph::new();
    let fft: BlockId = fg.add(Fft::new(16) as Fft)?.into();
    let sink: BlockId = fg.add(NullSink::<[Complex<f32>; 16]>::new())?.into();
    fg.stream_dyn(fft, "output", sink, "input")?;
    match Runtime::new().run(fg) {
        Err(Error::ValidationError(_)) => Ok(()),
        Err(e) => panic!("Expected ValidationError got {e:?}"),
        Ok(_) => panic!("Expected ValidationError got Ok(..)"),
    }
}

#[test]
fn message_invalid_in_port() -> Result<()> {
    let mut fg = Flowgraph::new();
    let source = MessageSource::new(Pmt::Ok, Duration::from_secs(1), Some(1));
    let source = fg.add(source)?;
    let sink = MessageSink::new();
    let sink = fg.add(sink)?;

    let result = fg.message(source, "out", sink, "non_existent");
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

    let result = fg.message(source, "fictitious", sink, "in");
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
fn stream_self_connection_is_rejected() -> Result<()> {
    let mut fg = Flowgraph::new();
    let copy = fg.add(Copy::<f32>::new())?;

    match fg.stream(&copy, |b| b.output(), &copy, |b| b.input()) {
        Err(Error::LockError) => Ok(()),
        Err(Error::ValidationError(msg)) => {
            assert!(msg.contains("self-connections"));
            Ok(())
        }
        Err(e) => panic!("Expected self-connection error got {e:?}"),
        Ok(_) => panic!("Expected self-connection error got Ok(..)"),
    }
}

#[test]
fn stream_duplicate_input_is_rejected_at_startup() -> Result<()> {
    let mut fg = Flowgraph::new();
    let src0 = fg.add(NullSource::<f32>::new())?;
    let src1 = fg.add(NullSource::<f32>::new())?;
    let snk = fg.add(NullSink::<f32>::new())?;

    fg.stream_dyn(src0, "output", snk, "input")?;
    fg.stream_dyn(src1, "output", snk, "input")?;

    match Runtime::new().run(fg) {
        Err(Error::ValidationError(msg)) => assert!(msg.contains("more than one connection")),
        Err(e) => panic!("Expected ValidationError got {e:?}"),
        Ok(_) => panic!("Expected ValidationError got Ok(..)"),
    }
    Ok(())
}

#[test]
fn single_reader_buffer_rejects_fanout_at_startup() -> Result<()> {
    let mut fg = Flowgraph::new();
    let src = fg.add(NullSource::<u8, slab::Writer<u8>>::new())?;
    let snk0 = fg.add(NullSink::<u8, slab::Reader<u8>>::new())?;
    let snk1 = fg.add(NullSink::<u8, slab::Reader<u8>>::new())?;

    fg.stream(&src, |b| b.output(), &snk0, |b| b.input())?;
    fg.stream(&src, |b| b.output(), &snk1, |b| b.input())?;

    match Runtime::new().run(fg) {
        Err(Error::ValidationError(msg)) => assert!(msg.contains("at most 1 reader")),
        Err(e) => panic!("Expected ValidationError got {e:?}"),
        Ok(_) => panic!("Expected ValidationError got Ok(..)"),
    }
    Ok(())
}

#[test]
fn stream_cycle_is_rejected_at_startup() -> Result<()> {
    let mut fg = Flowgraph::new();
    let a = fg.add(Copy::<f32>::new())?;
    let b = fg.add(Copy::<f32>::new())?;

    fg.stream(&a, |b| b.output(), &b, |b| b.input())?;
    fg.stream(&b, |b| b.output(), &a, |b| b.input())?;

    match Runtime::new().run(fg) {
        Err(Error::ValidationError(msg)) => assert!(msg.contains("directed acyclic graph")),
        Err(e) => panic!("Expected ValidationError got {e:?}"),
        Ok(_) => panic!("Expected ValidationError got Ok(..)"),
    }
    Ok(())
}

#[test]
fn stream_invalid_in_port() -> Result<()> {
    let mut fg = Flowgraph::new();
    let source = NullSource::<f32>::new();
    let source = fg.add(source)?;
    let sink = NullSink::<f32>::new();
    let sink = fg.add(sink)?;

    let result = fg.stream_dyn(source, "output", sink, "non_existent");
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

    let result = fg.stream_dyn(source, "fictitious", sink, "input");
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
