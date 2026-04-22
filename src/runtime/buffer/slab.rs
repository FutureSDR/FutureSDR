use std::any::Any;
use std::collections::VecDeque;
use std::mem::size_of;
use std::sync::Arc;
use std::sync::Mutex;

use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::ItemTag;
use crate::runtime::PortId;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::ConnectionState;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::buffer::CpuSample;
use crate::runtime::buffer::PortConfig;
use crate::runtime::buffer::PortCore;
use crate::runtime::buffer::PortEndpoint;
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
    core: PortCore,
    state: ConnectionState<ConnectedWriter<D>>,
    current: Option<CurrentBuffer<D>>,
    tags: Vec<ItemTag>,
}

#[derive(Debug)]
struct ConnectedWriter<D: CpuSample> {
    state: Arc<Mutex<State<D>>>,
    reserved_items: usize,
    reader: PortEndpoint,
}

impl<D> Writer<D>
where
    D: CpuSample,
{
    /// Create Slab writer
    pub fn new() -> Self {
        Self {
            core: PortCore::with_config(PortConfig::with_min_items(1)),
            state: ConnectionState::disconnected(),
            current: None,
            tags: Vec::new(),
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

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: crate::runtime::BlockInbox) {
        self.core.init(block_id, port_id, inbox);
    }

    fn validate(&self) -> Result<(), Error> {
        if self.state.is_connected() {
            Ok(())
        } else {
            Err(self.core.not_connected_error())
        }
    }

    fn connect(&mut self, dest: &mut Self::Reader) {
        let buffer_size_configured = self.core.min_buffer_size_in_items().is_some()
            || dest.core.min_buffer_size_in_items().is_some();
        let reserved_items = dest.core.min_items().unwrap_or(0);

        let mut min_items = if buffer_size_configured {
            let min_self = self.core.min_buffer_size_in_items().unwrap_or(0);
            let min_reader = dest.core.min_buffer_size_in_items().unwrap_or(0);
            std::cmp::max(min_self, min_reader)
        } else {
            config::config().buffer_size / size_of::<D>()
        };

        min_items = std::cmp::max(min_items, reserved_items + 1);

        let state = Arc::new(Mutex::new(State {
            writer_input: VecDeque::new(),
            reader_input: VecDeque::new(),
        }));
        let mut s = state.lock().unwrap();
        for _ in 0..4 {
            s.writer_input.push_back(BufferEmpty {
                buffer: vec![D::default(); min_items].into_boxed_slice(),
            });
        }
        drop(s);

        self.core
            .set_min_buffer_size_in_items(min_items - reserved_items);
        dest.core
            .set_min_buffer_size_in_items(min_items - reserved_items);

        self.state.set_connected(ConnectedWriter {
            state: state.clone(),
            reserved_items,
            reader: PortEndpoint::new(dest.core.inbox(), dest.core.port_id()),
        });
        dest.state.set_connected(ConnectedReader {
            state,
            reserved_items,
            writer: PortEndpoint::new(self.core.inbox(), self.core.port_id()),
        });
    }

    async fn notify_finished(&mut self) {
        let reserved_items = self.state.connected().reserved_items;
        if let Some(CurrentBuffer {
            buffer,
            offset,
            tags,
            ..
        }) = self.current.take()
            && offset > reserved_items
        {
            let mut state = self.state.connected().state.lock().unwrap();

            state.reader_input.push_back(BufferFull {
                buffer,
                items: offset - reserved_items,
                tags,
            });
        }

        let _ = self
            .state
            .connected()
            .reader
            .inbox()
            .send(BlockMessage::StreamInputDone {
                input_id: self.state.connected().reader.port_id(),
            })
            .await;
    }

    fn block_id(&self) -> BlockId {
        self.core.block_id()
    }

    fn port_id(&self) -> PortId {
        self.core.port_id()
    }
}

impl<D> CpuBufferWriter for Writer<D>
where
    D: CpuSample,
{
    type Item = D;

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags<'_>) {
        if self.current.is_none() {
            let mut state = self.state.connected().state.lock().unwrap();
            match state.writer_input.pop_front() {
                Some(b) => {
                    let end_offset = b.buffer.len();
                    self.current = Some(CurrentBuffer {
                        buffer: b.buffer,
                        offset: self.state.connected().reserved_items,
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

        let reserved_items = self.state.connected().reserved_items;
        let c = self.current.as_mut().unwrap();
        debug_assert!(n <= c.end_offset - c.offset);
        for t in self.tags.iter_mut() {
            t.index += c.offset;
        }
        c.tags.append(&mut self.tags);
        c.offset += n;
        if (c.end_offset - c.offset) < self.core.min_items().unwrap_or(1) {
            let c = self.current.take().unwrap();
            let mut state = self.state.connected().state.lock().unwrap();

            state.reader_input.push_back(BufferFull {
                buffer: c.buffer,
                items: c.offset - reserved_items,
                tags: c.tags,
            });

            self.state.connected().reader.inbox().notify();

            // make sure to be called again, if we have another buffer queued
            if !state.writer_input.is_empty() {
                self.core.inbox().notify();
            }
        }
    }

    fn set_min_items(&mut self, n: usize) {
        if self.state.is_connected() {
            warn!("set_min_items called after buffer is created. this has no effect");
        }
        self.core.set_min_items(n);
    }

    fn set_min_buffer_size_in_items(&mut self, n: usize) {
        if self.state.is_connected() {
            warn!(
                "set_min_buffer_size_in_items called after buffer is created. this has no effect"
            );
        }
        self.core.set_min_buffer_size_in_items(n);
    }
    fn max_items(&self) -> usize {
        self.core.min_buffer_size_in_items().unwrap_or(usize::MAX)
    }
}

/// Slab reader
#[derive(Debug)]
pub struct Reader<D: CpuSample> {
    core: PortCore,
    state: ConnectionState<ConnectedReader<D>>,
    current: Option<CurrentBuffer<D>>,
    finished: bool,
}

#[derive(Debug)]
struct ConnectedReader<D: CpuSample> {
    state: Arc<Mutex<State<D>>>,
    reserved_items: usize,
    writer: PortEndpoint,
}

impl<D> Reader<D>
where
    D: CpuSample,
{
    /// Create Slab Buffer Reader
    pub fn new() -> Self {
        Self {
            core: PortCore::new_disconnected(),
            state: ConnectionState::disconnected(),
            current: None,
            finished: false,
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

    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: crate::runtime::BlockInbox) {
        self.core.init(block_id, port_id, inbox);
    }
    fn validate(&self) -> Result<(), Error> {
        if self.state.is_connected() {
            Ok(())
        } else {
            Err(self.core.not_connected_error())
        }
    }
    async fn notify_finished(&mut self) {
        let _ = self
            .state
            .connected()
            .writer
            .inbox()
            .send(BlockMessage::StreamOutputDone {
                output_id: self.state.connected().writer.port_id(),
            })
            .await;
    }
    fn finish(&mut self) {
        self.finished = true;
    }
    fn finished(&self) -> bool {
        self.finished
            && self
                .state
                .as_ref()
                .is_none_or(|state| state.state.lock().unwrap().reader_input.is_empty())
    }
    fn block_id(&self) -> BlockId {
        self.core.block_id()
    }
    fn port_id(&self) -> PortId {
        self.core.port_id()
    }
}

impl<D> CpuBufferReader for Reader<D>
where
    D: CpuSample,
{
    type Item = D;

    fn slice_with_tags(&mut self) -> (&[Self::Item], &Vec<ItemTag>) {
        let reserved_items = self.state.as_ref().map_or(
            futuresdr::runtime::config::config().slab_reserved,
            |state| state.reserved_items,
        );
        if let Some(cur) = self.current.as_mut() {
            let left = cur.end_offset - cur.offset;
            debug_assert!(left > 0);
            if left <= reserved_items {
                let mut state = self.state.connected().state.lock().unwrap();
                if let Some(BufferFull {
                    mut buffer,
                    mut tags,
                    items,
                }) = state.reader_input.pop_front()
                {
                    buffer[(reserved_items - left)..reserved_items]
                        .clone_from_slice(&cur.buffer[cur.offset..(cur.offset + left)]);

                    for t in tags.iter_mut() {
                        t.index += left;
                    }
                    cur.tags.append(&mut tags);

                    let old = std::mem::replace(&mut cur.buffer, buffer);
                    state.writer_input.push_back(BufferEmpty { buffer: old });
                    self.state.connected().writer.inbox().notify();

                    cur.end_offset = reserved_items + items;
                    cur.offset = reserved_items - left;
                }
            }
        } else {
            let mut state = self.state.connected().state.lock().unwrap();
            match state.reader_input.pop_front() {
                Some(b) => {
                    let end_offset = b.items + reserved_items;
                    self.current = Some(CurrentBuffer {
                        buffer: b.buffer,
                        offset: reserved_items,
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

        let reserved_items = self.state.connected().reserved_items;
        let c = self.current.as_mut().unwrap();
        debug_assert!(n <= c.end_offset - c.offset);
        c.offset += n;

        if c.offset == c.end_offset {
            let b = self.current.take().unwrap();
            let mut state = self.state.connected().state.lock().unwrap();

            state
                .writer_input
                .push_back(BufferEmpty { buffer: b.buffer });

            self.state.connected().writer.inbox().notify();

            // make sure to be called again, if we have another buffer queued
            if !state.reader_input.is_empty() {
                self.core.inbox().notify();
            }
        // we call ourselfs again, since the buffer might be able to get merged
        } else if c.end_offset - c.offset <= reserved_items {
            let state = self.state.connected().state.lock().unwrap();
            if !state.reader_input.is_empty() {
                self.core.inbox().notify();
            }
        }
    }

    fn set_min_items(&mut self, n: usize) {
        if self.state.is_connected() {
            warn!("buffer size configured after buffer is connected. This has no effect");
        }
        self.core.set_min_items(n);
    }

    fn set_min_buffer_size_in_items(&mut self, n: usize) {
        if self.state.is_connected() {
            warn!("buffer size configured after buffer is connected. This has no effect");
        }
        self.core.set_min_buffer_size_in_items(n);
    }
    fn max_items(&self) -> usize {
        self.core.min_buffer_size_in_items().unwrap_or(usize::MAX)
    }
}
