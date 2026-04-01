use std::any::Any;
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::mem::size_of;
use std::sync::Arc;
use std::sync::Mutex;
use wgpu::BufferView;

use crate::runtime::BlockId;
use crate::runtime::BlockInbox;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::ItemTag;
use crate::runtime::PortId;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuSample;
use crate::runtime::buffer::wgpu::OutputBufferEmpty as BufferEmpty;
use crate::runtime::buffer::wgpu::OutputBufferFull as BufferFull;

#[derive(Debug)]
struct CurrentBuffer<D>
where
    D: CpuSample,
{
    buffer: *mut BufferFull<D>,
    byte_offset: usize,
    slice: BufferView,
}

// Needed for raw pointer `buffer`
unsafe impl<D> Send for CurrentBuffer<D> where D: CpuSample {}

/// Custom buffer writer
#[derive(Debug)]
pub struct Writer<D: CpuSample> {
    inbound: Arc<Mutex<Vec<BufferEmpty<D>>>>,
    outbound: Arc<Mutex<VecDeque<BufferFull<D>>>>,
    instance: Option<super::Instance>,
    writer_inbox: BlockInbox,
    writer_id: BlockId,
    writer_output_id: PortId,
    reader_inbox: BlockInbox,
    reader_input_id: PortId,
}

unsafe impl<D> Send for Writer<D> where D: CpuSample {}

impl<D> Writer<D>
where
    D: CpuSample,
{
    /// Create buffer writer
    pub fn new() -> Self {
        Writer {
            outbound: Arc::new(Mutex::new(VecDeque::new())),
            inbound: Arc::new(Mutex::new(Vec::new())),
            instance: None,
            writer_inbox: BlockInbox::default(),
            writer_id: BlockId::default(),
            writer_output_id: PortId::default(),
            reader_inbox: BlockInbox::default(),
            reader_input_id: PortId::default(),
        }
    }

    /// All available empty buffers
    pub fn buffers(&mut self) -> Vec<BufferEmpty<D>> {
        let mut vec = self.inbound.lock().unwrap();
        std::mem::take(&mut vec)
    }

    /// Set WGPU instance used to allocate reusable readback buffers.
    pub fn set_instance(&mut self, instance: super::Instance) {
        self.instance = Some(instance);
    }

    /// Inject reusable output readback buffers.
    pub fn inject_buffers_with_items(&mut self, n_buffers: usize, n_items: usize) {
        let Some(instance) = self.instance.as_ref() else {
            panic!("D2H writer: set_instance() must be called before injecting buffers");
        };
        let n_bytes = (n_items * size_of::<D>()) as u64;
        let mut inbound = self.inbound.lock().unwrap();
        for _ in 0..n_buffers {
            inbound.push(BufferEmpty {
                buffer: instance.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("d2h_output_buffer"),
                    size: n_bytes,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                _p: PhantomData,
            });
        }
    }

    /// Submit full buffer to downstream CPU reader
    pub fn submit(&mut self, buffer: BufferFull<D>) {
        self.outbound.lock().unwrap().push_back(buffer);
        self.reader_inbox.notify();
    }
}

impl<D> Default for Writer<D>
where
    D: CpuSample,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<D> BufferWriter for Writer<D>
where
    D: CpuSample,
{
    type Reader = Reader<D>;

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: crate::runtime::BlockInbox) {
        self.writer_id = block_id;
        self.writer_output_id = port_id;
        self.writer_inbox = inbox;
    }

    fn validate(&self) -> Result<(), Error> {
        if self.instance.is_none() {
            Err(Error::ValidationError(
                "D2H writer: no wgpu instance configured".to_string(),
            ))
        } else if !self.reader_inbox.is_closed() {
            Ok(())
        } else {
            Err(Error::ValidationError(format!(
                "{:?}:{:?} not connected",
                self.writer_id, self.writer_output_id
            )))
        }
    }

    fn connect(&mut self, dest: &mut Self::Reader) {
        dest.inbound = self.outbound.clone();
        dest.outbound = self.inbound.clone();
        dest.instance = self.instance.clone();
        dest.writer_output_id = self.writer_output_id.clone();
        dest.writer_inbox = self.writer_inbox.clone();

        self.reader_inbox = dest.reader_inbox.clone();
        self.reader_input_id = dest.reader_input_id.clone();
    }

    async fn notify_finished(&mut self) {
        let _ = self
            .reader_inbox
            .send(BlockMessage::StreamInputDone {
                input_id: self.reader_input_id.clone(),
            })
            .await;
    }

    fn block_id(&self) -> BlockId {
        self.writer_id
    }

    fn port_id(&self) -> PortId {
        self.writer_output_id.clone()
    }
}

/// Custom buffer reader
#[derive(Debug)]
pub struct Reader<D>
where
    D: CpuSample,
{
    buffer: Option<CurrentBuffer<D>>,
    inbound: Arc<Mutex<VecDeque<BufferFull<D>>>>,
    outbound: Arc<Mutex<Vec<BufferEmpty<D>>>>,
    writer_inbox: BlockInbox,
    writer_output_id: PortId,
    reader_id: BlockId,
    reader_input_id: PortId,
    reader_inbox: BlockInbox,
    instance: Option<super::Instance>,
    finished: bool,
}

unsafe impl<D> Send for Reader<D> where D: CpuSample {}

impl<D> Reader<D>
where
    D: CpuSample,
{
    /// Create Reader
    pub fn new() -> Self {
        Self {
            buffer: None,
            inbound: Arc::new(Mutex::new(VecDeque::new())),
            outbound: Arc::new(Mutex::new(Vec::new())),
            writer_inbox: BlockInbox::default(),
            writer_output_id: PortId::default(),
            reader_id: BlockId::default(),
            reader_input_id: PortId::default(),
            reader_inbox: BlockInbox::default(),
            instance: None,
            finished: false,
        }
    }

    /// Set WGPU instance.
    pub fn set_instance(&mut self, instance: super::Instance) {
        self.instance = Some(instance);
    }
}

impl<D> Default for Reader<D>
where
    D: CpuSample,
{
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<D> BufferReader for Reader<D>
where
    D: CpuSample,
{
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: crate::runtime::BlockInbox) {
        self.reader_id = block_id;
        self.reader_input_id = port_id;
        self.reader_inbox = inbox;
    }

    fn validate(&self) -> Result<(), Error> {
        if self.instance.is_none() {
            Err(Error::ValidationError(
                "D2H reader: no wgpu instance configured".to_string(),
            ))
        } else if !self.writer_inbox.is_closed() {
            Ok(())
        } else {
            Err(Error::ValidationError(format!(
                "{:?}:{:?} not connected",
                self.reader_id, self.reader_input_id
            )))
        }
    }

    async fn notify_finished(&mut self) {
        if self.finished {
            return;
        }

        let _ = self
            .writer_inbox
            .send(BlockMessage::StreamOutputDone {
                output_id: self.writer_output_id.clone(),
            })
            .await;
    }

    fn finish(&mut self) {
        self.finished = true;
    }

    fn finished(&self) -> bool {
        self.finished && self.inbound.lock().unwrap().is_empty()
    }

    fn block_id(&self) -> BlockId {
        self.reader_id
    }

    fn port_id(&self) -> PortId {
        self.reader_input_id.clone()
    }
}

impl<D> CpuBufferReader for Reader<D>
where
    D: CpuSample,
{
    type Item = D;

    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>) {
        static V: Vec<ItemTag> = vec![];
        debug!("D2H reader bytes");
        if self.buffer.is_none() {
            if let Some(b) = self.inbound.lock().unwrap().pop_front() {
                let buffer = Box::leak(Box::new(b));
                let t = buffer as *mut BufferFull<D>;
                let slice = buffer
                    .buffer
                    .slice(0..buffer.used_bytes as u64)
                    .get_mapped_range();
                self.buffer = Some(CurrentBuffer {
                    buffer: t,
                    byte_offset: 0,
                    slice,
                });
            } else {
                debug!("D2H reader return empty slice");
                return (&[], &V);
            }
        }
        debug!("D2H reader buffer available");

        unsafe {
            let buffer = self.buffer.as_ref().unwrap();
            let byte_len = buffer.slice.len();
            let ptr = buffer.slice.as_ptr();

            (
                std::slice::from_raw_parts(
                    ptr.add(buffer.byte_offset) as *const D,
                    (byte_len - buffer.byte_offset) / size_of::<D>(),
                ),
                &V,
            )
        }
    }

    fn consume(&mut self, amount: usize) {
        if amount == 0 {
            return;
        }
        debug_assert!(self.buffer.is_some());

        let buffer = self.buffer.as_mut().unwrap();
        let byte_len = buffer.slice.len();
        info!(
            "Consume -- byte_len: {}, offset: {}",
            byte_len, buffer.byte_offset
        );
        debug_assert!(amount * size_of::<D>() + buffer.byte_offset <= byte_len);

        buffer.byte_offset += amount * size_of::<D>();
        if buffer.byte_offset == byte_len {
            let c = unsafe { Box::from_raw(self.buffer.take().unwrap().buffer) };
            let buffer = c.buffer;
            buffer.unmap();
            self.outbound.lock().unwrap().push(BufferEmpty {
                buffer,
                _p: PhantomData,
            });
            self.writer_inbox.notify();

            // make sure to be called again for another potentially
            // queued buffer. could also check if there is one and only
            // message in this case.
            self.reader_inbox.notify();
        }
    }

    fn set_min_items(&mut self, _n: usize) {
        warn!("set_min_items not yet implemented for wgpu buffers");
    }

    fn set_min_buffer_size_in_items(&mut self, _n: usize) {
        warn!("set_min_buffer_size_in_items not yet implemented for wgpu buffers");
    }
    fn max_items(&self) -> usize {
        warn!("max_items not yet implemented for wgpu buffers");
        usize::MAX
    }
}
