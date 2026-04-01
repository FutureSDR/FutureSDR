use std::any::Any;
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::mem::size_of;
use std::sync::Arc;
use std::sync::Mutex;
use wgpu::BufferUsages;
use wgpu::BufferViewMut;

use crate::runtime::BlockId;
use crate::runtime::BlockInbox;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::ItemTag;
use crate::runtime::PortId;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::buffer::CpuSample;
use crate::runtime::buffer::Tags;
use crate::runtime::buffer::wgpu::InputBufferEmpty as BufferEmpty;
use crate::runtime::buffer::wgpu::InputBufferFull as BufferFull;

const UNMANAGED_SLOT_ID: usize = usize::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotState {
    WritableMapped,
    ReadyForGpu,
    Remapping,
}

#[derive(Debug)]
struct UploadSlot<D>
where
    D: CpuSample,
{
    buffer: wgpu::Buffer,
    capacity: usize,
    written_items: usize,
    state: SlotState,
    _p: PhantomData<D>,
}

#[derive(Debug)]
struct CurrentSlot {
    slot_id: usize,
    item_offset: usize,
    view: BufferViewMut,
}

/// Custom buffer writer
#[derive(Debug)]
pub struct Writer<D>
where
    D: CpuSample,
{
    current: Option<CurrentSlot>,
    slots: Arc<Mutex<Vec<UploadSlot<D>>>>,
    writable_ids: Arc<Mutex<Vec<usize>>>,
    ready_ids: Arc<Mutex<VecDeque<usize>>>,
    instance: Option<super::Instance>,
    writer_id: BlockId,
    writer_inbox: BlockInbox,
    writer_output_id: PortId,
    reader_inbox: BlockInbox,
    reader_input_id: PortId,
    tags: Vec<ItemTag>,
}

impl<D> Writer<D>
where
    D: CpuSample,
{
    /// Create buffer writer
    pub fn new() -> Self {
        Self {
            current: None,
            slots: Arc::new(Mutex::new(Vec::new())),
            writable_ids: Arc::new(Mutex::new(Vec::new())),
            ready_ids: Arc::new(Mutex::new(VecDeque::new())),
            instance: None,
            writer_id: BlockId::default(),
            writer_inbox: BlockInbox::default(),
            writer_output_id: PortId::default(),
            reader_inbox: BlockInbox::default(),
            reader_input_id: PortId::default(),
            tags: Vec::new(),
        }
    }

    /// Set WGPU instance used for staging buffer remap.
    pub fn set_instance(&mut self, instance: super::Instance) {
        self.instance = Some(instance);
    }

    /// Inject reusable mapped staging buffers.
    pub fn inject_buffers_with_items(&mut self, n_buffers: usize, n_items: usize) {
        let Some(instance) = self.instance.as_ref() else {
            panic!("H2D writer: set_instance() must be called before injecting buffers");
        };

        let n_bytes = (n_items * size_of::<D>()) as u64;
        let mut slots = self.slots.lock().unwrap();
        let mut writable_ids = self.writable_ids.lock().unwrap();

        for _ in 0..n_buffers {
            let slot_id = slots.len();
            slots.push(UploadSlot {
                buffer: instance.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("h2d_staging_buffer"),
                    size: n_bytes,
                    usage: BufferUsages::MAP_WRITE | BufferUsages::COPY_SRC,
                    mapped_at_creation: true,
                }),
                capacity: n_items,
                written_items: 0,
                state: SlotState::WritableMapped,
                _p: PhantomData,
            });
            writable_ids.push(slot_id);
        }
    }

    fn finalize_current(&mut self, used_items: usize) {
        let current = self.current.take().unwrap();
        let slot_id = current.slot_id;
        drop(current.view);

        {
            let mut slots = self.slots.lock().unwrap();
            let slot = slots.get_mut(slot_id).expect("H2D writer: invalid slot id");
            assert_eq!(
                slot.state,
                SlotState::WritableMapped,
                "H2D writer: finalize on non-writable slot"
            );
            slot.written_items = used_items;
            slot.state = SlotState::ReadyForGpu;
            slot.buffer.unmap();
        }

        self.ready_ids.lock().unwrap().push_back(slot_id);
    }

    fn acquire_current(&mut self) -> Option<()> {
        if self.current.is_some() {
            return Some(());
        }

        let slot_id = self.writable_ids.lock().unwrap().pop()?;

        let (capacity, view) = {
            let mut slots = self.slots.lock().unwrap();
            let slot = slots.get_mut(slot_id).expect("H2D writer: invalid slot id");
            assert_eq!(
                slot.state,
                SlotState::WritableMapped,
                "H2D writer: acquired non-writable slot"
            );
            slot.written_items = 0;
            let byte_len = (slot.capacity * size_of::<D>()) as u64;
            (
                slot.capacity,
                slot.buffer.slice(0..byte_len).get_mapped_range_mut(),
            )
        };

        self.current = Some(CurrentSlot {
            slot_id,
            item_offset: 0,
            view,
        });

        debug_assert!(capacity > 0);
        Some(())
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
                "H2D writer: no wgpu instance configured".to_string(),
            ))
        } else if self.reader_inbox.is_closed() {
            Err(Error::ValidationError(format!(
                "{:?}:{:?} not connected",
                self.writer_id, self.writer_output_id
            )))
        } else {
            Ok(())
        }
    }

    fn connect(&mut self, dest: &mut Self::Reader) {
        if self.instance.is_none() {
            self.instance = dest.instance.clone();
        }

        dest.slots = self.slots.clone();
        dest.ready_ids = self.ready_ids.clone();
        dest.writable_ids = self.writable_ids.clone();
        dest.instance = self.instance.clone();
        self.reader_input_id = dest.reader_input_id.clone();
        self.reader_inbox = dest.reader_inbox.clone();
        dest.writer_inbox = self.writer_inbox.clone();
        dest.writer_output_id = self.writer_output_id.clone();
    }

    async fn notify_finished(&mut self) {
        if let Some(current) = self.current.as_ref() {
            if current.item_offset > 0 {
                self.finalize_current(current.item_offset);
                self.reader_inbox.notify();
            } else {
                let current = self.current.take().unwrap();
                let slot_id = current.slot_id;
                drop(current.view);
                {
                    let mut slots = self.slots.lock().unwrap();
                    let slot = slots.get_mut(slot_id).expect("H2D writer: invalid slot id");
                    slot.written_items = 0;
                    slot.state = SlotState::WritableMapped;
                }
                self.writable_ids.lock().unwrap().push(slot_id);
            }
        }

        self.reader_inbox
            .send(BlockMessage::StreamInputDone {
                input_id: self.reader_input_id.clone(),
            })
            .await
            .unwrap();
    }

    fn block_id(&self) -> BlockId {
        self.writer_id
    }

    fn port_id(&self) -> PortId {
        self.writer_output_id.clone()
    }
}

#[async_trait]
impl<D> CpuBufferWriter for Writer<D>
where
    D: CpuSample,
{
    type Item = D;

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags<'_>) {
        if self.acquire_current().is_none() {
            return (&mut [], Tags::new(&mut self.tags, 0));
        }

        let current = self.current.as_mut().unwrap();
        let cap = {
            let slots = self.slots.lock().unwrap();
            slots[current.slot_id].capacity
        };
        let byte_offset = current.item_offset * size_of::<D>();
        let byte_end = cap * size_of::<D>();
        let mut tail_write_only = current.view.slice(byte_offset..byte_end);
        // `wgpu` 29 exposes mapped writes through a write-only view. FutureSDR's
        // writer API still expects a mutable slice here, so derive it from the
        // raw pointer while keeping the mapped view alive in `self.current`.
        let tail = unsafe {
            std::slice::from_raw_parts_mut(
                tail_write_only.as_raw_element_ptr().as_ptr(),
                byte_end - byte_offset,
            )
        };
        // Convert mapped bytes into typed sample slice with explicit alignment check.
        let (prefix, data, suffix) = unsafe { tail.align_to_mut::<D>() };
        assert!(
            prefix.is_empty() && suffix.is_empty(),
            "H2D writer: mapped buffer alignment invalid for sample type"
        );
        (data, Tags::new(&mut self.tags, 0))
    }

    fn produce(&mut self, amount: usize) {
        if amount == 0 {
            return;
        }

        let current = self.current.as_mut().unwrap();
        let item_capacity = {
            let slots = self.slots.lock().unwrap();
            slots[current.slot_id].capacity
        };
        assert!(
            amount + current.item_offset <= item_capacity,
            "H2D writer overflow: produce {} at offset {} exceeds capacity {}",
            amount,
            current.item_offset,
            item_capacity
        );
        current.item_offset += amount;
        if current.item_offset == item_capacity {
            self.finalize_current(item_capacity);
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

/// Custom buffer reader
#[derive(Debug)]
pub struct Reader<D>
where
    D: CpuSample,
{
    slots: Arc<Mutex<Vec<UploadSlot<D>>>>,
    ready_ids: Arc<Mutex<VecDeque<usize>>>,
    writable_ids: Arc<Mutex<Vec<usize>>>,
    instance: Option<super::Instance>,
    reader_id: BlockId,
    reader_input_id: PortId,
    reader_inbox: BlockInbox,
    writer_output_id: PortId,
    writer_inbox: BlockInbox,
    finished: bool,
}

impl<D> Reader<D>
where
    D: CpuSample,
{
    /// Create buffer reader
    pub fn new() -> Self {
        Self {
            slots: Arc::new(Mutex::new(Vec::new())),
            ready_ids: Arc::new(Mutex::new(VecDeque::new())),
            writable_ids: Arc::new(Mutex::new(Vec::new())),
            instance: None,
            reader_id: BlockId::default(),
            reader_input_id: PortId::default(),
            reader_inbox: BlockInbox::default(),
            writer_output_id: PortId::default(),
            writer_inbox: BlockInbox::default(),
            finished: false,
        }
    }

    /// Set WGPU instance used for remapping returned staging buffers.
    pub fn set_instance(&mut self, instance: super::Instance) {
        self.instance = Some(instance);
    }

    fn install_unmanaged_slot(&mut self, buffer: &BufferEmpty<D>) -> usize {
        let mut slots = self.slots.lock().unwrap();
        let slot_id = slots.len();
        slots.push(UploadSlot {
            buffer: buffer.buffer.clone(),
            capacity: buffer.capacity,
            written_items: 0,
            state: SlotState::Remapping,
            _p: PhantomData,
        });
        slot_id
    }

    /// Send empty buffer back to writer.
    pub fn submit(&mut self, buffer: BufferEmpty<D>) {
        let Some(instance) = self.instance.clone() else {
            panic!("H2D reader: set_instance() must be called before submit");
        };

        let slot_id = if buffer.slot_id == UNMANAGED_SLOT_ID {
            self.install_unmanaged_slot(&buffer)
        } else {
            buffer.slot_id
        };

        let (buffer_for_map, capacity) = {
            let mut slots = self.slots.lock().unwrap();
            let slot = slots.get_mut(slot_id).expect("H2D reader: invalid slot id");
            assert_eq!(
                slot.state,
                SlotState::Remapping,
                "H2D reader: submit on non-remapping slot"
            );
            if slot.capacity != buffer.capacity {
                warn!(
                    "H2D reader: capacity mismatch on submit (slot {} has {}, submit has {})",
                    slot_id, slot.capacity, buffer.capacity
                );
            }
            (slot.buffer.clone(), slot.capacity)
        };

        let writable_ids = self.writable_ids.clone();
        let slots_arc = self.slots.clone();
        let writer_inbox = self.writer_inbox.clone();
        let byte_len = (capacity * size_of::<D>()) as u64;
        let slice = buffer_for_map.slice(0..byte_len);
        slice.map_async(wgpu::MapMode::Write, move |result| match result {
            Ok(()) => {
                {
                    let mut slots = slots_arc.lock().unwrap();
                    let slot = slots
                        .get_mut(slot_id)
                        .expect("H2D reader: invalid slot id in map callback");
                    slot.written_items = 0;
                    slot.state = SlotState::WritableMapped;
                }
                writable_ids.lock().unwrap().push(slot_id);
                writer_inbox.notify();
            }
            Err(e) => {
                warn!(
                    "H2D reader: map_async(write) failed for slot {}: {:?}",
                    slot_id, e
                );
            }
        });

        // Non-blocking kick to help callback progress without stalling this thread.
        let _ = instance.device.poll(wgpu::PollType::Poll);
    }

    /// Get full buffer
    pub fn get_buffer(&mut self) -> Option<BufferFull<D>> {
        let slot_id = self.ready_ids.lock().unwrap().pop_front()?;
        let mut slots = self.slots.lock().unwrap();
        let slot = slots.get_mut(slot_id).expect("H2D reader: invalid slot id");
        assert_eq!(
            slot.state,
            SlotState::ReadyForGpu,
            "H2D reader: get_buffer on non-ready slot"
        );
        slot.state = SlotState::Remapping;
        Some(BufferFull {
            buffer: slot.buffer.clone(),
            n_items: slot.written_items,
            capacity: slot.capacity,
            slot_id,
            _p: PhantomData,
        })
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
                "H2D reader: no wgpu instance configured".to_string(),
            ))
        } else if self.writer_inbox.is_closed() {
            Err(Error::ValidationError(format!(
                "{:?}:{:?} not connected",
                self.reader_id, self.reader_input_id
            )))
        } else {
            Ok(())
        }
    }

    async fn notify_finished(&mut self) {
        if self.finished {
            return;
        }

        self.writer_inbox
            .send(BlockMessage::StreamOutputDone {
                output_id: self.writer_output_id.clone(),
            })
            .await
            .unwrap();
    }

    fn finish(&mut self) {
        self.finished = true;
    }

    fn finished(&self) -> bool {
        self.finished && self.ready_ids.lock().unwrap().is_empty()
    }

    fn block_id(&self) -> BlockId {
        self.reader_id
    }

    fn port_id(&self) -> PortId {
        self.reader_input_id.clone()
    }
}
