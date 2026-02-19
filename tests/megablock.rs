use anyhow::Result;
use futuresdr::blocks::Copy;
use futuresdr::blocks::MessageCopy;
use futuresdr::blocks::MessageSink;
use futuresdr::blocks::MessageSource;
use futuresdr::blocks::StreamDuplicator;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;
use futuresdr::runtime::Error;
use std::time::Duration;

#[derive(MegaBlock)]
#[stream_inputs(
    in0: DefaultCpuReader<u32> = "a.input",
    in1: DefaultCpuReader<u32> = "b.input"
)]
#[stream_outputs(
    out0: DefaultCpuWriter<u32> = "a.output",
    out1: DefaultCpuWriter<u32> = "b.output"
)]
struct MultiStreamMega {
    a: Option<BlockRef<Copy<u32>>>,
    b: Option<BlockRef<Copy<u32>>>,
}

impl MegaBlock for MultiStreamMega {
    fn add_megablock(mut self, fg: &mut Flowgraph) -> Result<Self, Error> {
        let a = Copy::<u32>::new();
        let b = Copy::<u32>::new();
        connect!(fg, a; b);
        self.a = Some(a);
        self.b = Some(b);
        Ok(self)
    }
}

#[test]
fn stream_megablock_two_inputs_two_outputs() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src0 = VectorSource::<u32>::new(vec![1, 2, 3, 4]);
    let src1 = VectorSource::<u32>::new(vec![10, 20, 30, 40]);
    let mega = MultiStreamMega { a: None, b: None };
    let snk0 = VectorSink::<u32>::new(4);
    let snk1 = VectorSink::<u32>::new(4);

    connect!(fg, src0 > in0.mega.out0 > snk0; src1 > in1.mega.out1 > snk1);
    Runtime::new().run(fg)?;

    let snk0 = snk0.get()?;
    assert_eq!(snk0.items(), &[1, 2, 3, 4]);
    let snk1 = snk1.get()?;
    assert_eq!(snk1.items(), &[10, 20, 30, 40]);
    Ok(())
}

#[test]
fn stream_megablock_dyn_connect() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src0 = VectorSource::<u32>::new(vec![1, 2, 3, 4]);
    let src1 = VectorSource::<u32>::new(vec![10, 20, 30, 40]);
    let mega = MultiStreamMega { a: None, b: None };
    let snk0 = VectorSink::<u32>::new(4);
    let snk1 = VectorSink::<u32>::new(4);

    let src0 = fg.add(src0)?;
    let src1 = fg.add(src1)?;
    let mega = fg.add(mega)?;
    let snk0 = fg.add(snk0)?;
    let snk1 = fg.add(snk1)?;

    fg.connect_dyn(
        src0.dyn_stream_output("output")?,
        mega.dyn_stream_input("in0")?,
    )?;
    fg.connect_dyn(
        src1.dyn_stream_output("output")?,
        mega.dyn_stream_input("in1")?,
    )?;
    fg.connect_dyn(
        mega.dyn_stream_output("out0")?,
        snk0.dyn_stream_input("input")?,
    )?;
    fg.connect_dyn(
        mega.dyn_stream_output("out1")?,
        snk1.dyn_stream_input("input")?,
    )?;

    Runtime::new().run(fg)?;

    let snk0 = snk0.get()?;
    assert_eq!(snk0.items(), &[1, 2, 3, 4]);
    let snk1 = snk1.get()?;
    assert_eq!(snk1.items(), &[10, 20, 30, 40]);
    Ok(())
}

#[derive(MegaBlock)]
#[stream_inputs(in0: DefaultCpuReader<u32> = "dup.input")]
#[stream_outputs(
    out0: DefaultCpuWriter<u32> = "dup.outputs[0]",
    out1: DefaultCpuWriter<u32> = "dup.outputs[1]"
)]
struct MultiOutMega {
    dup: Option<BlockRef<StreamDuplicator<u32, 2>>>,
}

impl MegaBlock for MultiOutMega {
    fn add_megablock(mut self, fg: &mut Flowgraph) -> Result<Self, Error> {
        let dup = StreamDuplicator::<u32, 2>::new();
        connect!(fg, dup);
        self.dup = Some(dup);
        Ok(self)
    }
}

#[test]
fn stream_megablock_indexed_outputs() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = VectorSource::<u32>::new(vec![1, 2, 3, 4]);
    let mega = MultiOutMega { dup: None };
    let snk0 = VectorSink::<u32>::new(4);
    let snk1 = VectorSink::<u32>::new(4);

    connect!(fg, src > in0.mega.out0 > snk0; src > in0.mega.out1 > snk1);
    Runtime::new().run(fg)?;

    let snk0 = snk0.get()?;
    assert_eq!(snk0.items(), &[1, 2, 3, 4]);
    let snk1 = snk1.get()?;
    assert_eq!(snk1.items(), &[1, 2, 3, 4]);
    Ok(())
}

#[derive(MegaBlock)]
#[stream_inputs(in0: I = "copy.input")]
#[stream_outputs(out0: O = "copy.output")]
struct GenericCopyMega<T, I, O>
where
    T: std::marker::Copy + Send + Sync + 'static,
    I: CpuBufferReader<Item = T> + Default + 'static,
    O: CpuBufferWriter<Item = T> + Default + 'static,
{
    copy: Option<BlockRef<Copy<T, I, O>>>,
}

impl<T, I, O> MegaBlock for GenericCopyMega<T, I, O>
where
    T: std::marker::Copy + Send + Sync + 'static,
    I: CpuBufferReader<Item = T> + Default + 'static,
    O: CpuBufferWriter<Item = T> + Default + 'static,
{
    fn add_megablock(mut self, fg: &mut Flowgraph) -> Result<Self, Error> {
        let copy = Copy::<T, I, O>::new();
        connect!(fg, copy);
        self.copy = Some(copy);
        Ok(self)
    }
}

#[test]
fn stream_megablock_generic_copy_io() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = VectorSource::<u32>::new(vec![1, 2, 3, 4]);
    let mega = GenericCopyMega::<u32, DefaultCpuReader<u32>, DefaultCpuWriter<u32>> { copy: None };
    let snk = VectorSink::<u32>::new(4);

    connect!(fg, src > in0.mega.out0 > snk);
    Runtime::new().run(fg)?;

    let snk = snk.get()?;
    assert_eq!(snk.items(), &[1, 2, 3, 4]);
    Ok(())
}

#[derive(MegaBlock)]
#[message_inputs(r#in = "copy.in")]
#[message_outputs(out = "copy.out")]
struct MessageMega {
    copy: Option<BlockRef<MessageCopy>>,
}

impl MegaBlock for MessageMega {
    fn add_megablock(mut self, fg: &mut Flowgraph) -> Result<Self, Error> {
        let copy = MessageCopy::new();
        connect!(fg, copy);
        self.copy = Some(copy);
        Ok(self)
    }
}

#[test]
fn message_megablock_input_output() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = MessageSource::new(Pmt::Ok, Duration::from_secs(1), Some(1));
    let mega = MessageMega { copy: None };
    let snk = MessageSink::new();

    connect!(fg, src | mega | snk);
    Runtime::new().run(fg)?;

    let snk = snk.get()?;
    assert_eq!(snk.received(), 1);
    Ok(())
}
