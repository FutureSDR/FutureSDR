use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::Flowgraph;
use futuresdr::prelude::Result;
use futuresdr::prelude::Runtime;
use futuresdr::prelude::connect;
use futuresdr::runtime::__private::KernelInterface;
use futuresdr::runtime::buffer::CpuBufferReader;
use futuresdr::runtime::buffer::CpuBufferWriter;
use futuresdr::runtime::buffer::DefaultCpuReader;
use futuresdr::runtime::buffer::DefaultCpuWriter;
use futuresdr::runtime::buffer::InplaceReader;
use futuresdr::runtime::buffer::InplaceWriter;
use futuresdr::runtime::buffer::LocalCpuReader;
use futuresdr::runtime::buffer::LocalCpuWriter;
use futuresdr::runtime::buffer::ThreadSafeConnect;
use futuresdr::runtime::buffer::circuit;
use futuresdr::runtime::dev::SendKernel;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PlainSample {
    Active,
    #[default]
    Idle,
}

fn assert_send_kernel<T: SendKernel>() {}
fn assert_send<T: Send>() {}
fn assert_thread_safe_connect<T: ThreadSafeConnect>() {}
fn assert_kernel_interface<T: KernelInterface>() {}
fn assert_local_cpu_reader<T: CpuBufferReader>() {}
fn assert_local_cpu_writer<T: CpuBufferWriter>() {}
fn assert_inplace_reader<T: InplaceReader>() {}
fn assert_inplace_writer<T: InplaceWriter>() {}

#[test]
fn normal_and_local_types_use_the_same_traits() {
    assert_send::<DefaultCpuReader<u8>>();
    assert_send::<DefaultCpuWriter<u8>>();
    assert_thread_safe_connect::<DefaultCpuWriter<u8>>();
    assert_local_cpu_reader::<DefaultCpuReader<u8>>();
    assert_local_cpu_writer::<DefaultCpuWriter<u8>>();
    assert_local_cpu_reader::<LocalCpuReader<u8>>();
    assert_local_cpu_writer::<LocalCpuWriter<u8>>();
    assert_inplace_reader::<circuit::Reader<i32>>();
    assert_inplace_writer::<circuit::Writer<i32>>();
    #[cfg(not(target_arch = "wasm32"))]
    assert_send::<circuit::Reader<i32>>();
    #[cfg(not(target_arch = "wasm32"))]
    assert_send::<circuit::Writer<i32>>();
    assert_thread_safe_connect::<circuit::Writer<i32>>();
}

#[test]
fn derived_block_interface_supports_local_buffers() {
    assert_kernel_interface::<VectorSource<u8, LocalCpuWriter<u8>>>();
    assert_send_kernel::<VectorSource<u8, DefaultCpuWriter<u8>>>();
}

#[test]
fn plain_enum_samples_use_default_cpu_buffers() -> Result<()> {
    let samples = vec![PlainSample::Active, PlainSample::Idle, PlainSample::Active];
    let mut fg = Flowgraph::new();
    let src = VectorSource::<PlainSample>::new(samples.clone());
    let snk = VectorSink::<PlainSample>::new(samples.len());

    connect!(fg, src > snk);

    let fg = Runtime::new().run(fg)?;
    assert_eq!(fg.block(&snk)?.items(), &samples);
    Ok(())
}
