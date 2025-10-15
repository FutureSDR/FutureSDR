//! Buffer Implementations for CPU and Accelerator Memory

// ==================== BURN =======================
#[cfg(feature = "burn")]
pub mod burn;

/// In-place circuit buffer
pub mod circuit;

/// Double-mapped circular buffer
#[cfg(not(target_arch = "wasm32"))]
pub mod circular;

// ===================== SLAB ========================
/// Slab buffer
pub mod slab;

// ==================== VULKAN =======================
#[cfg(feature = "vulkan")]
pub mod vulkan;

// ==================== WGPU =======================
#[cfg(feature = "wgpu")]
pub mod wgpu;

// -==================== ZYNQ ========================
#[cfg(all(feature = "zynq", target_os = "linux"))]
pub mod zynq;

use std::any::Any;
use std::future::Future;

use futuresdr::channel::mpsc::Sender;
use futuresdr::runtime::BlockId;
use futuresdr::runtime::BlockMessage;
use futuresdr::runtime::Error;
use futuresdr::runtime::ItemTag;
use futuresdr::runtime::PortId;
use futuresdr::runtime::Tag;

/// The most generic buffer reader
///
/// This is the core trait that every buffer reader has to implements.
/// It is what the runtime needs to make things work.
#[async_trait]
pub trait BufferReader: Any {
    /// for downcasting
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Initialize buffer
    ///
    /// This sets the own block ID, Port ID, and message receiver so that it can
    /// be communicated the the other end when making connections.
    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: Sender<BlockMessage>);
    /// Check if connected
    fn validate(&self) -> Result<(), Error>;
    /// notify upstream that we are done
    async fn notify_finished(&mut self);
    /// The upstream is done
    ///
    /// The Block will usually process the remaining samples and shut down.
    fn finish(&mut self);
    /// Did the upstream already mark this buffer as done.
    fn finished(&self) -> bool;
    /// Own Block ID
    fn block_id(&self) -> BlockId;
    /// Own Port ID
    fn port_id(&self) -> PortId;
}

/// The most generic buffer writer
///
/// This is the core trait that every buffer writer has to implements.
/// It is what the runtime needs to make things work.
pub trait BufferWriter {
    /// The corresponding reader.
    type Reader: BufferReader;
    /// Initialize buffer
    ///
    /// This sets the own block ID, Port ID, and message receiver so that it can
    /// be communicated the the other end when making connections.
    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: Sender<BlockMessage>);
    /// Check if connected
    fn validate(&self) -> Result<(), Error>;
    /// Connect the writer to (another) reader.
    fn connect(&mut self, dest: &mut Self::Reader);
    /// Connect the writer to (another) reader.
    fn connect_dyn(&mut self, dest: &mut dyn BufferReader) -> Result<(), Error> {
        if let Some(concrete) = dest.as_any_mut().downcast_mut::<Self::Reader>() {
            self.connect(concrete);
            Ok(())
        } else {
            Err(Error::ValidationError(
                "dyn BufferReader has wrong type".to_string(),
            ))
        }
    }
    /// Notify downstream blocks that we are done.
    fn notify_finished(&mut self) -> impl Future<Output = ()> + Send;
    /// Own Block ID
    fn block_id(&self) -> BlockId;
    /// Own Port ID
    fn port_id(&self) -> PortId;
}

/// A short hand for the traits required for CpuSamples
pub trait CpuSample: Default + Clone + std::fmt::Debug + Send + Sync + 'static {}

impl<T> CpuSample for T where T: Default + Clone + std::fmt::Debug + Send + Sync + 'static {}

/// A generic CPU buffer reader (out-of-place)
pub trait CpuBufferReader: BufferReader + Default + Send {
    /// Buffer Items
    type Item: CpuSample;
    /// Get available samples.
    fn slice(&mut self) -> &[Self::Item] {
        self.slice_with_tags().0
    }
    /// Get available samples and tags.
    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>);
    /// Consume Items
    fn consume(&mut self, n: usize);
    /// Configure the minimum numer of items required in [work()](futuresdr::runtime::Kernel::work)
    ///
    /// This defines the minimum number of samples that the block needs to proceed. For example, an
    /// FFT block requires samples correspoding to the FFT size.
    fn set_min_items(&mut self, n: usize);
    /// Configure the minimum buffer size
    ///
    /// This sets the minimum number of samples that the buffer can take. This is independent from
    /// any requirements in [work()](futuresdr::runtime::Kernel::work) but mainly for performance reasons, i.e., it
    /// defines the tradeoff between throughput and latency.
    ///
    /// By default, it will be set to the value defined in the [config](futuresdr::config::Config).
    fn set_min_buffer_size_in_items(&mut self, n: usize);
    /// Maximum number of items that fit in the buffer
    fn max_items(&self) -> usize;
}

/// A generic CPU buffer writer (out-of-place)
///
/// Current upstream implemenations are a circular buffer with douple mapping
/// and the SLAB buffer
pub trait CpuBufferWriter: BufferWriter + Default + Send {
    /// Buffer Items
    type Item: CpuSample;
    /// Available buffer space
    fn slice(&mut self) -> &mut [Self::Item] {
        self.slice_with_tags().0
    }
    /// Available buffer space and tags.
    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags<'_>);
    /// Produce items
    fn produce(&mut self, n: usize);
    /// Configure the minimum numer of items required in [work()](futuresdr::runtime::Kernel::work)
    ///
    /// This defines the minimum number of samples that the block needs to proceed. For example, an
    /// FFT block requires samples correspoding to the FFT size.
    fn set_min_items(&mut self, n: usize);
    /// Configure the minimum buffer size
    ///
    /// This sets the minimum number of samples that the buffer can take. This is independent from
    /// any requirements in [work()](futuresdr::runtime::Kernel::work) but mainly for performance reasons, i.e., it
    /// defines the tradeoff between throughput and latency.
    ///
    /// By default, it will be set to the value defined in the [config](futuresdr::config::Config).
    fn set_min_buffer_size_in_items(&mut self, n: usize);
    /// Maximum number of items that fit in the buffer
    fn max_items(&self) -> usize;
}

/// In-Place Buffer
pub trait InplaceBuffer {
    /// Type of the samples in the buffer
    type Item: CpuSample;
    /// Set number of valid samples
    fn set_valid(&mut self, valid: usize);
    /// Items in the buffer
    fn slice(&mut self) -> &mut [Self::Item];
    /// Items in the buffer and tags
    fn slice_with_tags(&mut self) -> (&mut [Self::Item], &mut Vec<ItemTag>);
}

/// In-Place Reader
pub trait InplaceReader: BufferReader + Default + Send {
    /// Items in the reader
    type Item: CpuSample;
    /// Buffer type
    type Buffer: InplaceBuffer<Item = Self::Item>;

    /// Get next buffer
    fn get_full_buffer(&mut self) -> Option<Self::Buffer>;
    /// Has more full buffers
    fn has_more_buffers(&mut self) -> bool;
    /// Put an empty buffer to circle it back to the beginning of the circuit
    fn put_empty_buffer(&mut self, buffer: Self::Buffer);
    /// Notify the circuit start that we consumed a buffer
    fn notify_consumed_buffer(&mut self);
}

/// In-Place Writer
pub trait InplaceWriter: BufferWriter + Default + Send {
    /// Items in the writer
    type Item: CpuSample;
    /// Buffer type
    type Buffer: InplaceBuffer<Item = Self::Item>;

    /// Put full buffer
    fn put_full_buffer(&mut self, buffer: Self::Buffer);

    /// Get empty buffer
    ///
    /// This is typically used in sources, i.e., when there is no inplace reader
    fn get_empty_buffer(&mut self) -> Option<Self::Buffer>;
    /// Has more empty buffers
    fn has_more_buffers(&mut self) -> bool;
    /// Inject new buffers
    fn inject_buffers(&mut self, n_buffers: usize) {
        let n_items =
            futuresdr::runtime::config::config().buffer_size / std::mem::size_of::<Self::Item>();
        self.inject_buffers_with_items(n_buffers, n_items);
    }
    /// Inject new buffers
    fn inject_buffers_with_items(&mut self, n_buffers: usize, n_items: usize);
}

#[cfg(not(target_arch = "wasm32"))]
/// Default [CpuBufferReader] implementation
pub type DefaultCpuReader<D> = circular::Reader<D>;
/// Default [CpuBufferWriter] implementation
#[cfg(not(target_arch = "wasm32"))]
pub type DefaultCpuWriter<D> = circular::Writer<D>;
#[cfg(target_arch = "wasm32")]
/// Default [CpuBufferReader] implementation
pub type DefaultCpuReader<D> = slab::Reader<D>;
#[cfg(target_arch = "wasm32")]
/// Default [CpuBufferWriter] implementation
pub type DefaultCpuWriter<D> = slab::Writer<D>;

/// Output Tags
pub struct Tags<'a> {
    tags: &'a mut Vec<ItemTag>,
    offset: usize,
}

impl<'a> Tags<'a> {
    /// Create Output Tags structure
    ///
    /// Should only be constructed in buffer implementations.
    pub fn new(tags: &'a mut Vec<ItemTag>, offset: usize) -> Self {
        Self { tags, offset }
    }
    /// Used in work to add a tag to the output
    pub fn add_tag(&mut self, index: usize, tag: Tag) {
        self.tags.push(ItemTag {
            index: index + self.offset,
            tag,
        });
    }
}
