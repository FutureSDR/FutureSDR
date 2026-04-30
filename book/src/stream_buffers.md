# Stream Buffers

Stream buffers move samples between connected stream ports. A source block writes into the writer side of a buffer, and the downstream block reads from the reader side.

FutureSDR can be extended with arbitrary buffer implementations. At the lowest level, a buffer only has to provide a writer and reader pair implementing `BufferWriter` and `BufferReader`. Those two traits are mostly the type-erased runtime connection layer: they let the flowgraph connect a writer port to a reader port, validate the connection, and propagate termination.

Buffer implementations can expose their own higher-level API. A CPU buffer exposes slices. A GPU buffer can expose GPU resources. A DMA buffer can expose hardware-owned memory. FutureSDR therefore provides specialized traits for the common buffer families instead of forcing every buffer into one sample-slice API.

The main stream buffer trait families are:

- `BufferWriter` / `BufferReader`: minimal base trait that all buffers implement.
- `CpuBufferWriter` / `CpuBufferReader`: out-of-place CPU buffer API.
- `InplaceWriter` / `InplaceReader` / `InplaceBuffer`: in-place CPU buffer API.

Most application code should use the default buffers through existing blocks. You only need to name buffer types when you want a non-default transport, such as in-place, GPU, or DMA buffers.

## Normal Buffers

Normal CPU stream buffers are the default for most blocks. They expose readable and writable slices:

```rust
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::prelude::*;

let mut fg = Flowgraph::new();

let src = NullSource::<f32>::new();
let head = Head::<f32>::new(1024);
let snk = NullSink::<f32>::new();

connect!(fg, src > head > snk);
```

On native targets, `DefaultCpuReader<T>` and `DefaultCpuWriter<T>` are double-mapped circular buffers. They avoid wrapping logic in the hot path while still behaving like a ring buffer.

On WebAssembly, the default CPU buffer is the `Slab` implementation. It uses ordinary allocated slabs because double-mapped virtual memory is not available in the browser environment.

The default buffer size is controlled by the runtime config key `buffer_size`; see [Running Applications](running_apps.md#configuration). Some blocks also configure minimum item counts internally. For example, an FFT block needs enough samples for one transform.

You can select another CPU buffer by naming the buffer generic parameters:

```rust
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::slab;

let mut fg = Flowgraph::new();

let src = NullSource::<f32, slab::Writer<f32>>::new();
let head = Head::<f32, slab::Reader<f32>, slab::Writer<f32>>::new(1024);
let snk = NullSink::<f32, slab::Reader<f32>>::new();

connect!(fg, src > head > snk);
```

This is rarely needed in normal applications, but it is useful for benchmarks or platform-specific experiments.

## In-Place Buffers

Normal stream buffers copy data from an input slice to an output slice when a block transforms samples. In-place buffers move owned buffer chunks through the flowgraph instead. A block can mutate the chunk and pass the same allocation downstream.

This can help for simple transformations, such as adding a constant to every sample, where copying between input and output buffers would dominate the work.

In-place buffers have a different API from normal CPU buffers:

- `InplaceReader::get_full_buffer()` receives a full reusable buffer chunk.
- `InplaceWriter::put_full_buffer()` forwards the same chunk after processing.
- `InplaceBuffer::slice()` gives mutable access to the chunk contents.

That means in-place processing usually needs blocks written for the in-place API. See the [in-place example](https://github.com/FutureSDR/FutureSDR/tree/main/examples/inplace) for complete source.

Reusable buffers need to return to the start of the pipeline. FutureSDR models this as a circuit. First connect the forward stream edges as usual, then close the circuit from the source to the final sink with `<`:

```rust
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::circuit;

let mut fg = Flowgraph::new();

let mut src: VectorSource<i32> = VectorSource::new(vec![1, 2, 3, 4]);
src.output().inject_buffers(4);

let apply = Apply::new();
let snk = VectorSink::new(4);

connect!(fg, src > apply > snk);
connect!(fg, src < snk);
```

The `<` connection closes the return path for empty buffers. The source injects a fixed number of reusable buffers, processing blocks mutate and forward them, and the sink returns each consumed buffer to the source side.

This concept is inspired by [qsdr](https://github.com/daniestevez/qsdr), which also explores in-place work APIs for SDR-style flowgraphs.

In-place buffers also implement the CPU buffer traits. This allows hybrid graphs where standard CPU blocks sit at the boundary and an in-place block processes the middle:

```rust
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::circuit;

let mut fg = Flowgraph::new();

let mut src = VectorSource::<i32, circuit::Writer<i32>>::new(vec![1, 2, 3, 4]);
src.output().inject_buffers(4);

let apply = Apply::new();
let snk = VectorSink::new(4);

connect!(fg, src > apply > snk);
connect!(fg, src < snk);
```

Here the standard `VectorSource` writes into a circuit writer, the in-place `Apply` block mutates the buffer chunk, and the standard `VectorSink` reads from a circuit reader.

## Accelerator Buffers

Accelerator buffers use the same connection model but expose APIs that match their hardware or framework:

- Xilinx Zynq DMA buffers move chunks through AXI DMA-backed memory.
- WGPU buffers use [`wgpu`](https://wgpu.rs/) resources and can run in native or browser environments.
- Vulkan buffers use Vulkan storage buffers.
- Burn buffers use [Burn](https://burn.dev/) tensors for machine-learning workloads.

These buffer APIs are intentionally not standardized beyond `BufferWriter` and `BufferReader`. A GPU block may need mapped buffers. A DMA block may need hardware buffer handles. A tensor buffer may need framework-specific tensor ownership.

Accelerator buffer implementations typically also implement CPU buffer traits at the host boundary:

- Host-to-device writers implement `CpuBufferWriter`, so a CPU source can write samples into an upload buffer.
- Device-to-host readers implement `CpuBufferReader`, so a CPU sink can read processed samples after download.

For example, the WGPU example uses a CPU `VectorSource` with an `H2DWriter`, a GPU processing block, and a CPU `VectorSink` with a `D2HReader`:

```rust
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::blocks::Wgpu;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::wgpu;
use futuresdr::runtime::buffer::wgpu::D2HReader;
use futuresdr::runtime::buffer::wgpu::H2DWriter;

let mut fg = Flowgraph::new();

let src = VectorSource::<f32, H2DWriter<f32>>::new(vec![1.0, 2.0, 3.0]);
let instance = wgpu::Instance::new().await;
let gpu = Wgpu::new(instance, 4096, 4, 4);
let snk = VectorSink::<f32, D2HReader<f32>>::new(3);

connect!(fg, src > gpu > snk);
```

See the complete accelerator examples:

- [WGPU example](https://github.com/FutureSDR/FutureSDR/tree/main/examples/wgpu)
- [Vulkan example](https://github.com/FutureSDR/FutureSDR/tree/main/examples/vulkan)
- [Zynq example](https://github.com/FutureSDR/FutureSDR/tree/main/examples/zynq)
- [Burn example](https://github.com/FutureSDR/FutureSDR/tree/main/examples/burn)

See the [API Docs](https://docs.rs/futuresdr/latest/futuresdr/runtime/buffer/index.html) for more details.
