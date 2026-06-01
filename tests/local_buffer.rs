use futuresdr::blocks::VectorSource;
use futuresdr::runtime::__private::KernelInterface;
use futuresdr::runtime::__private::SendKernelInterface;
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
