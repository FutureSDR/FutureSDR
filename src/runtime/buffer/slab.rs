use futures::prelude::*;
use std::any::Any;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;

use crate::channel::mpsc::Sender;
use crate::channel::mpsc::channel;
use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::ItemTag;
use crate::runtime::PortId;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::buffer::CpuSample;
use crate::runtime::buffer::Tags;
use crate::runtime::config;

#[derive(Debug)]
struct BufferEmpty<D: CpuSample> {
    buffer: Box<[D]>,
}

#[derive(Debug)]
struct BufferFull<D: CpuSample> {
    buffer: Box<[D]>,
    /// number of items, starting at reserved space
    items: usize,
    tags: Vec<ItemTag>,
}

#[derive(Debug)]
struct CurrentBuffer<D: CpuSample> {
    buffer: Box<[D]>,
    end_offset: usize,
    offset: usize,
    tags: Vec<ItemTag>,
}

#[derive(Debug)]
struct State<D: CpuSample> {
    writer_input: VecDeque<BufferEmpty<D>>,
    reader_input: VecDeque<BufferFull<D>>,
}

/// Slab writer
#[derive(Debug)]
pub struct Writer<D: CpuSample> {
    current: Option<CurrentBuffer<D>>,
    state: Arc<Mutex<State<D>>>,
    reserved_items: usize,
    reader_inbox: Sender<BlockMessage>,
    reader_input_id: PortId,
    inbox: Sender<BlockMessage>,
    port_id: PortId,
    block_id: BlockId,
    tags: Vec<ItemTag>,
    min_items: usize,
    min_buffer_size_in_items: Option<usize>,
}

impl<D> Writer<D>
where
    D: CpuSample,
{
    /// Create Slab writer
    pub fn new() -> Self {
        let (rx, _) = channel(0);
        Self {
            current: None,
            state: Arc::new(Mutex::new(State {
                writer_input: VecDeque::new(),
                reader_input: VecDeque::new(),
            })),
            reserved_items: 0,
            reader_inbox: rx.clone(),
            reader_input_id: PortId::default(),
            inbox: rx,
            port_id: PortId::default(),
            block_id: BlockId(0),
            tags: Vec::new(),
            min_items: 1,
            min_buffer_size_in_items: None,
        }
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

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: Sender<BlockMessage>) {
        self.block_id = block_id;
        self.port_id = port_id;
        self.inbox = inbox;
    }

    fn validate(&self) -> Result<(), Error> {
        if !self.reader_inbox.is_closed() {
            Ok(())
        } else {
            Err(Error::ValidationError(format!(
                "{:?}:{:?} not connected",
                self.block_id, self.port_id
            )))
        }
    }

    fn connect(&mut self, dest: &mut Self::Reader) {
        let buffer_size_configured =
            self.min_buffer_size_in_items.is_some() || dest.min_buffer_size_in_items.is_some();

        let reserved_items = dest.min_items.unwrap_or(0);

        let mut min_items = if buffer_size_configured {
            let min_self = self.min_buffer_size_in_items.unwrap_or(0);
            let min_reader = dest.min_buffer_size_in_items.unwrap_or(0);
            std::cmp::max(min_self, min_reader)
        } else {
            config::config().buffer_size / size_of::<D>()
        };

        min_items = std::cmp::max(min_items, reserved_items + 1);

        let mut s = self.state.lock().unwrap();
        for _ in 0..4 {
            s.writer_input.push_back(BufferEmpty {
                buffer: vec![D::default(); min_items].into_boxed_slice(),
            });
        }

        self.min_buffer_size_in_items = Some(min_items - reserved_items);
        dest.min_buffer_size_in_items = Some(min_items - reserved_items);

        self.reader_inbox = dest.reader_inbox.clone();
        self.reader_input_id = dest.port_id();
        self.reserved_items = reserved_items;

        dest.state = self.state.clone();
        dest.reserved_items = reserved_items;
        dest.writer_inbox = self.inbox.clone();
        dest.writer_output_id = self.port_id();
    }

    async fn notify_finished(&mut self) {
        if let Some(CurrentBuffer {
            buffer,
            offset,
            tags,
            ..
        }) = self.current.take()
        {
            if offset > self.reserved_items {
                let mut state = self.state.lock().unwrap();

                state.reader_input.push_back(BufferFull {
                    buffer,
                    items: offset - self.reserved_items,
                    tags,
                });
            }
        }

        let _ = self
            .reader_inbox
            .send(BlockMessage::StreamInputDone {
                input_id: self.reader_input_id.clone(),
            })
            .await;
    }

    fn block_id(&self) -> BlockId {
        self.block_id
    }

    fn port_id(&self) -> PortId {
        self.port_id.clone()
    }
}

impl<D> CpuBufferWriter for Writer<D>
where
    D: CpuSample,
{
    type Item = D;

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags) {
        if self.current.is_none() {
            let mut state = self.state.lock().unwrap();
            match state.writer_input.pop_front() {
                Some(b) => {
                    let end_offset = b.buffer.len();
                    self.current = Some(CurrentBuffer {
                        buffer: b.buffer,
                        offset: self.reserved_items,
                        end_offset,
                        tags: Vec::new(),
                    });
                }
                _ => {
                    return (&mut [], Tags::new(&mut self.tags, 0));
                }
            }
        }

        let c = self.current.as_mut().unwrap();

        (&mut c.buffer[c.offset..], Tags::new(&mut self.tags, 0))
    }

    fn produce(&mut self, n: usize) {
        if n == 0 {
            return;
        }

        let c = self.current.as_mut().unwrap();
        debug_assert!(n <= c.end_offset - c.offset);
        for t in self.tags.iter_mut() {
            t.index += c.offset;
        }
        c.tags.append(&mut self.tags);
        c.offset += n;
        if (c.end_offset - c.offset) < self.min_items {
            let c = self.current.take().unwrap();
            let mut state = self.state.lock().unwrap();

            state.reader_input.push_back(BufferFull {
                buffer: c.buffer,
                items: c.offset - self.reserved_items,
                tags: c.tags,
            });

            let _ = self.reader_inbox.try_send(BlockMessage::Notify);

            // make sure to be called again, if we have another buffer queued
            if !state.writer_input.is_empty() {
                let _ = self.inbox.try_send(BlockMessage::Notify);
            }
        }
    }

    fn set_min_items(&mut self, n: usize) {
        if !self.reader_inbox.is_closed() {
            warn!("set_min_items called after buffer is created. this has no effect");
        }
        self.min_items = n;
    }

    fn set_min_buffer_size_in_items(&mut self, n: usize) {
        if !self.reader_inbox.is_closed() {
            warn!(
                "set_min_buffer_size_in_items called after buffer is created. this has no effect"
            );
        }
        self.min_buffer_size_in_items = Some(n);
    }
    fn max_items(&self) -> usize {
        self.min_buffer_size_in_items.unwrap_or(usize::MAX)
    }
}

/// Slab reader
#[derive(Debug)]
pub struct Reader<D: CpuSample> {
    current: Option<CurrentBuffer<D>>,
    state: Arc<Mutex<State<D>>>,
    reserved_items: usize,
    reader_inbox: Sender<BlockMessage>,
    block_id: BlockId,
    port_id: PortId,
    writer_inbox: Sender<BlockMessage>,
    writer_output_id: PortId,
    finished: bool,
    min_items: Option<usize>,
    min_buffer_size_in_items: Option<usize>,
}

impl<D> Reader<D>
where
    D: CpuSample,
{
    /// Create Slab Buffer Reader
    pub fn new() -> Self {
        let (reader_inbox, _) = channel(0);
        let (writer_inbox, _) = channel(0);
        Self {
            current: None,
            state: Arc::new(Mutex::new(State {
                writer_input: VecDeque::new(),
                reader_input: VecDeque::new(),
            })),
            reserved_items: futuresdr::runtime::config::config().slab_reserved,
            reader_inbox,
            writer_inbox,
            finished: false,
            block_id: BlockId(0),
            port_id: PortId::default(),
            writer_output_id: PortId::default(),
            min_items: None,
            min_buffer_size_in_items: None,
        }
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

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: Sender<BlockMessage>) {
        self.block_id = block_id;
        self.port_id = port_id;
        self.reader_inbox = inbox;
    }
    fn validate(&self) -> Result<(), Error> {
        if !self.writer_inbox.is_closed() {
            Ok(())
        } else {
            Err(Error::ValidationError(format!(
                "{:?}:{:?} not connected",
                self.block_id, self.port_id
            )))
        }
    }
    async fn notify_finished(&mut self) {
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
        self.finished && self.state.lock().unwrap().reader_input.is_empty()
    }
    fn block_id(&self) -> BlockId {
        self.block_id
    }
    fn port_id(&self) -> PortId {
        self.port_id.clone()
    }
}

impl<D> CpuBufferReader for Reader<D>
where
    D: CpuSample,
{
    type Item = D;

    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>) {
        if let Some(cur) = self.current.as_mut() {
            let left = cur.end_offset - cur.offset;
            debug_assert!(left > 0);
            if left <= self.reserved_items {
                let mut state = self.state.lock().unwrap();
                if let Some(BufferFull {
                    mut buffer,
                    mut tags,
                    items,
                }) = state.reader_input.pop_front()
                {
                    buffer[(self.reserved_items - left)..self.reserved_items]
                        .clone_from_slice(&cur.buffer[cur.offset..(cur.offset + left)]);

                    for t in tags.iter_mut() {
                        t.index += left;
                    }
                    cur.tags.append(&mut tags);

                    let old = std::mem::replace(&mut cur.buffer, buffer);
                    state.writer_input.push_back(BufferEmpty { buffer: old });
                    let _ = self.writer_inbox.try_send(BlockMessage::Notify);

                    cur.end_offset = self.reserved_items + items;
                    cur.offset = self.reserved_items - left;
                }
            }
        } else {
            let mut state = self.state.lock().unwrap();
            match state.reader_input.pop_front() {
                Some(b) => {
                    let end_offset = b.items + self.reserved_items;
                    self.current = Some(CurrentBuffer {
                        buffer: b.buffer,
                        offset: self.reserved_items,
                        end_offset,
                        tags: b.tags,
                    });
                }
                _ => {
                    static V: Vec<ItemTag> = vec![];
                    return (&[], &V);
                }
            }
        }

        let c = self.current.as_mut().unwrap();
        (&c.buffer[c.offset..c.end_offset], &c.tags)
    }

    fn consume(&mut self, n: usize) {
        if n == 0 {
            return;
        }

        let c = self.current.as_mut().unwrap();
        debug_assert!(n <= c.end_offset - c.offset);
        c.offset += n;

        if c.offset == c.end_offset {
            let b = self.current.take().unwrap();
            let mut state = self.state.lock().unwrap();

            state
                .writer_input
                .push_back(BufferEmpty { buffer: b.buffer });

            let _ = self.writer_inbox.try_send(BlockMessage::Notify);

            // make sure to be called again, if we have another buffer queued
            if !state.reader_input.is_empty() {
                let _ = self.reader_inbox.try_send(BlockMessage::Notify);
            }
        // we call ourselfs again, since the buffer might be able to get merged
        } else if c.end_offset - c.offset <= self.reserved_items {
            let state = self.state.lock().unwrap();
            if !state.reader_input.is_empty() {
                let _ = self.reader_inbox.try_send(BlockMessage::Notify);
            }
        }
    }

    fn set_min_items(&mut self, n: usize) {
        if !self.writer_inbox.is_closed() {
            warn!("buffer size configured after buffer is connected. This has no effect");
        }
        self.min_items = Some(n);
    }

    fn set_min_buffer_size_in_items(&mut self, n: usize) {
        if !self.writer_inbox.is_closed() {
            warn!("buffer size configured after buffer is connected. This has no effect");
        }
        self.min_buffer_size_in_items = Some(n);
    }
    fn max_items(&self) -> usize {
        self.min_buffer_size_in_items.unwrap_or(usize::MAX)
    }
}
