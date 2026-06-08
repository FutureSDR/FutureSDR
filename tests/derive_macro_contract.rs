use std::cell::Cell;
use std::rc::Rc;

use futuresdr::runtime::__private::KernelInterface;
use futuresdr::runtime::BlockId;
use futuresdr::runtime::Error;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::PortId;
use futuresdr::runtime::dev::prelude::*;

#[derive(Block)]
#[null_kernel]
struct PortShape {
    #[input]
    input_vec: Vec<DefaultCpuReader<u8>>,
    #[input]
    input_arr: [DefaultCpuReader<u8>; 2],
    #[input]
    input_tuple: (DefaultCpuReader<u8>, DefaultCpuReader<u8>),
    #[output]
    output_vec: Vec<DefaultCpuWriter<u8>>,
    #[output]
    output_arr: [DefaultCpuWriter<u8>; 2],
    #[output]
    output_tuple: (DefaultCpuWriter<u8>, DefaultCpuWriter<u8>),
}

impl PortShape {
    fn new() -> Self {
        fn reader() -> DefaultCpuReader<u8> {
            Default::default()
        }
        fn writer() -> DefaultCpuWriter<u8> {
            Default::default()
        }

        Self {
            input_vec: vec![reader(), reader()],
            input_arr: std::array::from_fn(|_| reader()),
            input_tuple: (reader(), reader()),
            output_vec: vec![writer(), writer()],
            output_arr: std::array::from_fn(|_| writer()),
            output_tuple: (writer(), writer()),
        }
    }
}

#[test]
fn derive_expands_vector_array_and_tuple_stream_port_names() {
    let mut block = PortShape::new();

    let mut inputs = Vec::new();
    block
        .visit_stream_inputs(&mut |port, _| {
            inputs.push(port.name().to_string());
            Ok(())
        })
        .unwrap();
    assert_eq!(
        inputs,
        [
            "input_vec[0]",
            "input_vec[1]",
            "input_arr[0]",
            "input_arr[1]",
            "input_tuple.0",
            "input_tuple.1",
        ]
    );

    let mut outputs = Vec::new();
    block
        .visit_stream_outputs(&mut |port, _| {
            outputs.push(port.name().to_string());
            Ok(())
        })
        .unwrap();
    assert_eq!(
        outputs,
        [
            "output_vec[0]",
            "output_vec[1]",
            "output_arr[0]",
            "output_arr[1]",
            "output_tuple.0",
            "output_tuple.1",
        ]
    );

    assert!(
        block
            .with_stream_input(&PortId::from("input_tuple.1"), |_| ())
            .is_ok()
    );
    assert!(
        block
            .with_stream_output(&PortId::from("output_arr[0]"), |_| ())
            .is_ok()
    );
}

#[derive(Block)]
#[message_inputs(ping = "renamed-ping", plain)]
#[message_outputs(out, done)]
struct MessageContract {
    seen: Vec<Pmt>,
}

impl Kernel for MessageContract {}

impl MessageContract {
    async fn ping(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> futuresdr::runtime::Result<Pmt> {
        self.seen.push(p);
        Ok(Pmt::Usize(self.seen.len()))
    }

    async fn plain(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> futuresdr::runtime::Result<Pmt> {
        self.seen.push(p);
        Ok(Pmt::Ok)
    }
}

#[test]
fn derive_exposes_message_metadata_and_dispatches_handlers() {
    assert_eq!(
        MessageContract::message_inputs(),
        &["renamed-ping", "plain"]
    );
    assert_eq!(MessageContract::message_outputs(), &["out", "done"]);

    let mut block = MessageContract { seen: Vec::new() };
    let mut io = WorkIo {
        call_again: false,
        finished: false,
    };
    let mut mo = MessageOutputs::new(BlockId(0), vec!["out".to_string(), "done".to_string()]);
    let mut meta = BlockMeta::new();

    let ret = futuresdr::runtime::block_on(block.call_handler(
        &mut io,
        &mut mo,
        &mut meta,
        PortId::from("renamed-ping"),
        Pmt::U32(7),
    ))
    .unwrap();
    assert_eq!(ret, Pmt::Usize(1));
    assert_eq!(block.seen, vec![Pmt::U32(7)]);

    let err = futuresdr::runtime::block_on(block.call_handler(
        &mut io,
        &mut mo,
        &mut meta,
        PortId::from("missing"),
        Pmt::Null,
    ))
    .unwrap_err();
    assert!(matches!(err, Error::InvalidMessagePort(_, port) if port.name() == "missing"));
}

#[derive(Block)]
#[message_outputs(out)]
struct NonSendMessageSource {
    _state: Rc<Cell<usize>>,
}

impl Kernel for NonSendMessageSource {}

#[derive(Block)]
#[message_inputs(input)]
struct MessageSink;

impl Kernel for MessageSink {}

impl MessageSink {
    async fn input(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        _p: Pmt,
    ) -> futuresdr::runtime::Result<Pmt> {
        Ok(Pmt::Ok)
    }
}

#[test]
fn flowgraph_validates_message_outputs_from_cached_static_metadata() {
    let mut fg = Flowgraph::new();
    let domain = fg.local_domain().unwrap();
    let src = fg
        .add_local(domain, || NonSendMessageSource {
            _state: Rc::new(Cell::new(0)),
        })
        .unwrap();
    let dst = fg.add(MessageSink).unwrap();

    let err = fg.message(src, "missing", dst, "input").unwrap_err();
    assert!(matches!(err, Error::InvalidMessagePort(_, port) if port.name() == "missing"));

    fg.message(src, "out", dst, "input").unwrap();
}

#[derive(Block)]
#[null_kernel]
struct NonSendLocalBlock {
    state: Rc<Cell<usize>>,
}

#[test]
fn non_send_blocks_can_be_added_to_local_domains() {
    let mut fg = Flowgraph::new();
    let domain = fg.local_domain().unwrap();
    let block = fg
        .add_local(domain, || NonSendLocalBlock {
            state: Rc::new(Cell::new(42)),
        })
        .unwrap();

    let value = block.with(&fg, |block| block.state.get()).unwrap();
    assert_eq!(value, 42);
}
