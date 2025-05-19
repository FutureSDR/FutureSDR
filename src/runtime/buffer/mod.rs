//! Buffer Implementations for CPU and Accelerator Memory

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

// // -==================== ZYNQ ========================
#[cfg(feature = "zynq")]
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
    fn finished(&mut self) -> bool;
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

/// A generic CPU buffer reader (out-of-place)
pub trait CpuBufferReader: BufferReader + Default + Send {
    /// Buffer Items
    type Item;
    /// Get available samples.
    fn slice(&mut self) -> &[Self::Item];
    /// Get available samples and tags.
    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>);
    /// Consume Items
    fn consume(&mut self, n: usize);
}

/// A generic CPU buffer writer (out-of-place)
///
/// Current upstream implemenations are a circular buffer with douple mapping
/// and the SLAB buffer
pub trait CpuBufferWriter: BufferWriter + Default + Send {
    /// Buffer Items
    type Item;
    /// Available buffer space
    fn slice(&mut self) -> &mut [Self::Item];
    /// Available buffer space and tags.
    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags);
    /// samples produced
    fn produce(&mut self, n: usize);
}

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
            index: self.offset + index,
            tag,
        });
    }
}
