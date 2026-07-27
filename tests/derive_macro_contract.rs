use std::cell::Cell;
use std::rc::Rc;

use futuresdr::runtime::__private::KernelInterface;
use futuresdr::runtime::BlockId;
use futuresdr::runtime::Error;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::PortId;
use futuresdr::runtime::PortIndex;
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
    let mut index = 0;
    while let Some((name, _)) = block.stream_input_at(PortIndex::new(index)) {
        inputs.push(name.into_string());
        index += 1;
    }
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
    let mut index = 0;
    while let Some((name, _)) = block.stream_output_at(PortIndex::new(index)) {
        outputs.push(name.into_string());
        index += 1;
    }
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

    assert!(block.stream_input_at(PortIndex::new(5)).is_some());
    assert!(block.stream_input_at(PortIndex::new(6)).is_none());
    assert!(block.stream_output_at(PortIndex::new(2)).is_some());
    assert!(block.stream_output_at(PortIndex::new(6)).is_none());
}

#[derive(Block)]
#[message_inputs(r#await)]
#[message_outputs(r#loop)]
struct RawPortNames {
    #[input]
    r#type: DefaultCpuReader<u8>,
    #[output]
    r#async: DefaultCpuWriter<u8>,
}

impl Kernel for RawPortNames {}

impl RawPortNames {
    async fn r#await(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        _p: Pmt,
    ) -> futuresdr::runtime::Result<Pmt> {
        Ok(Pmt::Ok)
    }
}

#[test]
fn derive_strips_raw_identifier_prefixes_from_port_names() {
    let mut block = RawPortNames {
        r#type: Default::default(),
        r#async: Default::default(),
    };

    let (input_name, _) = block.stream_input_at(PortIndex::new(0)).unwrap();
    assert_eq!(input_name.as_str(), "type");
    assert!(block.stream_input_at(PortIndex::new(1)).is_none());

    let (output_name, _) = block.stream_output_at(PortIndex::new(0)).unwrap();
    assert_eq!(output_name.as_str(), "async");
    assert!(block.stream_output_at(PortIndex::new(1)).is_none());
    assert_eq!(RawPortNames::message_inputs(), &["await"]);
    assert_eq!(RawPortNames::message_outputs(), &["loop"]);
    assert_eq!(
        RawPortNames::message_input_id("r#await"),
        Some(futuresdr::runtime::PortIndex::new(0))
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
        _meta: &BlockMeta,
        p: Pmt,
    ) -> futuresdr::runtime::Result<Pmt> {
        self.seen.push(p);
        Ok(Pmt::Usize(self.seen.len()))
    }

    async fn plain(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
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
    let mut mo = MessageOutputs::new(BlockId(0), &["out", "done"]);
    let meta = BlockMeta::new();

    let ret = futuresdr::runtime::block_on(block.call_handler(
        &mut io,
        &mut mo,
        &meta,
        MessageContract::message_input_id("renamed-ping").unwrap(),
        Pmt::U32(7),
    ))
    .unwrap();
    assert_eq!(ret, Pmt::Usize(1));

    let ret = futuresdr::runtime::block_on(block.call_handler(
        &mut io,
        &mut mo,
        &meta,
        futuresdr::runtime::PortIndex::new(1),
        Pmt::U32(9),
    ))
    .unwrap();
    assert_eq!(ret, Pmt::Ok);
    assert_eq!(block.seen, vec![Pmt::U32(7), Pmt::U32(9)]);

    let err = futuresdr::runtime::block_on(block.call_handler(
        &mut io,
        &mut mo,
        &meta,
        futuresdr::runtime::PortIndex::new(99),
        Pmt::Null,
    ))
    .unwrap_err();
    assert!(matches!(err, Error::InvalidMessagePort(_, port) if port == PortId::index(99)));
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
        _meta: &BlockMeta,
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
        .with_local_domain(domain, |ctx| {
            Ok(ctx.add(NonSendMessageSource {
                _state: Rc::new(Cell::new(0)),
            }))
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
        .with_local_domain(domain, |ctx| {
            Ok(ctx.add(NonSendLocalBlock {
                state: Rc::new(Cell::new(42)),
            }))
        })
        .unwrap();

    let value = block.with(&fg, |block| block.state.get()).unwrap();
    assert_eq!(value, 42);
}
