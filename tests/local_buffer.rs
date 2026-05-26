use anyhow::Result;
use futuresdr::blocks::VectorSource;
use futuresdr::runtime::__private::KernelInterface;
use futuresdr::runtime::__private::SendKernelInterface;
use futuresdr::runtime::BlockId;
use futuresdr::runtime::PortId;
use futuresdr::runtime::buffer::BufferReader;
use futuresdr::runtime::buffer::BufferWriter;
use futuresdr::runtime::buffer::CpuBufferReader;
use futuresdr::runtime::buffer::CpuBufferWriter;
use futuresdr::runtime::buffer::DefaultCpuReader;
use futuresdr::runtime::buffer::DefaultCpuWriter;
use futuresdr::runtime::buffer::InplaceReader;
use futuresdr::runtime::buffer::InplaceWriter;
use futuresdr::runtime::buffer::LocalCpuReader;
use futuresdr::runtime::buffer::LocalCpuWriter;
use futuresdr::runtime::buffer::SendCpuBufferReader;
use futuresdr::runtime::buffer::SendCpuBufferWriter;
#[cfg(not(target_arch = "wasm32"))]
use futuresdr::runtime::buffer::SendInplaceReader;
#[cfg(not(target_arch = "wasm32"))]
use futuresdr::runtime::buffer::SendInplaceWriter;
use futuresdr::runtime::buffer::circuit;
use futuresdr::runtime::dev::Kernel;
use futuresdr::runtime::dev::LocalBlockInbox;
use futuresdr::runtime::dev::SendKernel;

struct TestKernel;

impl Kernel for TestKernel {}

fn assert_send_kernel<T: SendKernel>() {}
fn assert_cpu_reader<T: SendCpuBufferReader>() {}
fn assert_cpu_writer<T: SendCpuBufferWriter>() {}
fn assert_kernel_interface<T: KernelInterface>() {}
fn assert_send_kernel_interface<T: SendKernelInterface>() {}
fn assert_local_cpu_reader<T: CpuBufferReader>() {}
fn assert_local_cpu_writer<T: CpuBufferWriter>() {}
fn assert_inplace_reader<T: InplaceReader>() {}
fn assert_inplace_writer<T: InplaceWriter>() {}
#[cfg(not(target_arch = "wasm32"))]
fn assert_send_inplace_reader<T: SendInplaceReader>() {}
#[cfg(not(target_arch = "wasm32"))]
fn assert_send_inplace_writer<T: SendInplaceWriter>() {}

#[test]
fn normal_and_local_types_use_the_same_traits() {
    assert_send_kernel::<TestKernel>();
    assert_cpu_reader::<DefaultCpuReader<u8>>();
    assert_cpu_writer::<DefaultCpuWriter<u8>>();
    assert_local_cpu_reader::<DefaultCpuReader<u8>>();
    assert_local_cpu_writer::<DefaultCpuWriter<u8>>();
    assert_local_cpu_reader::<LocalCpuReader<u8>>();
    assert_local_cpu_writer::<LocalCpuWriter<u8>>();
    assert_inplace_reader::<circuit::Reader<i32>>();
    assert_inplace_writer::<circuit::Writer<i32>>();
    #[cfg(not(target_arch = "wasm32"))]
    assert_send_inplace_reader::<circuit::Reader<i32>>();
    #[cfg(not(target_arch = "wasm32"))]
    assert_send_inplace_writer::<circuit::Writer<i32>>();
}

#[test]
fn derived_block_interface_supports_local_buffers() {
    assert_kernel_interface::<VectorSource<u8, LocalCpuWriter<u8>>>();
    assert_send_kernel_interface::<VectorSource<u8, DefaultCpuWriter<u8>>>();
}

#[test]
fn local_cpu_buffer_moves_items() -> Result<()> {
    let mut writer = LocalCpuWriter::<u8>::default();
    let mut reader = LocalCpuReader::<u8>::default();

    BufferWriter::init(
        &mut writer,
        BlockId(0),
        PortId::new("out"),
        LocalBlockInbox::default(),
    );
    BufferReader::init(
        &mut reader,
        BlockId(1),
        PortId::new("in"),
        LocalBlockInbox::default(),
    );

    CpuBufferWriter::set_min_buffer_size_in_items(&mut writer, 5);
    CpuBufferReader::set_min_items(&mut reader, 1);
    BufferWriter::connect(&mut writer, &mut reader);

    BufferWriter::validate(&writer)?;
    BufferReader::validate(&reader)?;

    let out = CpuBufferWriter::slice(&mut writer);
    out[..4].copy_from_slice(&[1, 2, 3, 4]);
    CpuBufferWriter::produce(&mut writer, 4);
    futuresdr::runtime::block_on(BufferWriter::notify_finished(&mut writer));

    let input = CpuBufferReader::slice(&mut reader);
    assert_eq!(input, &[1, 2, 3, 4]);

    CpuBufferReader::consume(&mut reader, 4);
    assert!(CpuBufferReader::slice(&mut reader).is_empty());

    Ok(())
}

#[test]
fn local_cpu_buffer_flushes_partial_buffer_on_finish() -> Result<()> {
    let mut writer = LocalCpuWriter::<u8>::default();
    let mut reader = LocalCpuReader::<u8>::default();

    BufferWriter::init(
        &mut writer,
        BlockId(0),
        PortId::new("out"),
        LocalBlockInbox::default(),
    );
    BufferReader::init(
        &mut reader,
        BlockId(1),
        PortId::new("in"),
        LocalBlockInbox::default(),
    );

    CpuBufferWriter::set_min_buffer_size_in_items(&mut writer, 8);
    BufferWriter::connect(&mut writer, &mut reader);

    let out = CpuBufferWriter::slice(&mut writer);
    out[..3].copy_from_slice(&[9, 8, 7]);
    CpuBufferWriter::produce(&mut writer, 3);
    assert!(CpuBufferReader::slice(&mut reader).is_empty());

    futuresdr::runtime::block_on(BufferWriter::notify_finished(&mut writer));

    let input = CpuBufferReader::slice(&mut reader);
    assert_eq!(input, &[9, 8, 7]);

    Ok(())
}
