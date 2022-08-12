use futures::channel::mpsc::Sender;
use futures::prelude::*;
use std::any::Any;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::runtime::buffer::BufferBuilder;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferReaderHost;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::BufferWriterHost;
use crate::runtime::config;
use crate::runtime::BlockMessage;
use crate::runtime::ItemTag;

#[derive(Debug, PartialEq, Hash)]
pub struct Slab {
    min_bytes: usize,
    n_buffer: usize,
    reserved_items: usize,
}

impl Eq for Slab {}

impl Slab {
    pub fn new() -> Slab {
        Slab {
            min_bytes: config::config().buffer_size,
            n_buffer: 2,
            reserved_items: config::config().slab_reserved,
        }
    }

    pub fn with_size(min_bytes: usize) -> Slab {
        Slab {
            min_bytes,
            n_buffer: 2,
            reserved_items: config::config().slab_reserved,
        }
    }

    pub fn with_buffers(n_buffer: usize) -> Slab {
        Slab {
            min_bytes: config::config().buffer_size,
            n_buffer,
            reserved_items: config::config().slab_reserved,
        }
    }

    pub fn with_config(min_bytes: usize, n_buffer: usize, reserved_items: usize) -> Slab {
        Slab {
            min_bytes,
            n_buffer,
            reserved_items,
        }
    }
}

impl Default for Slab {
    fn default() -> Self {
        Self::new()
    }
}

impl BufferBuilder for Slab {
    fn build(
        &self,
        item_size: usize,
        writer_inbox: Sender<BlockMessage>,
        writer_output_id: usize,
    ) -> BufferWriter {
        Writer::new(
            item_size,
            self.min_bytes,
            self.n_buffer,
            self.reserved_items,
            writer_inbox,
            writer_output_id,
        )
    }
}

#[derive(Debug)]
struct BufferEmpty {
    buffer: Box<[u8]>,
}

#[derive(Debug)]
struct BufferFull {
    buffer: Box<[u8]>,
    items: usize,
    tags: Vec<ItemTag>,
}

// everything is measured in items, e.g., offsets, capacity, space available

#[derive(Debug)]
struct CurrentBuffer {
    buffer: Box<[u8]>,
    offset: usize,
    capacity: usize,
    tags: Vec<ItemTag>,
}

#[derive(Debug)]
pub struct Writer {
    current: Option<CurrentBuffer>,
    state: Arc<Mutex<State>>,
    item_size: usize,
    reserved_items: usize,
    reader_inbox: Option<Sender<BlockMessage>>,
    reader_input_id: Option<usize>,
    writer_inbox: Sender<BlockMessage>,
    writer_output_id: usize,
    finished: bool,
}

#[derive(Debug)]
struct State {
    writer_input: VecDeque<BufferEmpty>,
    reader_input: VecDeque<BufferFull>,
}

impl Writer {
    pub fn new(
        item_size: usize,
        min_bytes: usize,
        n_buffer: usize,
        reserved_items: usize,
        writer_inbox: Sender<BlockMessage>,
        writer_output_id: usize,
    ) -> BufferWriter {
        let mut buffer_size = min_bytes;
        while buffer_size % item_size != 0 {
            buffer_size += 1;
        }

        let mut writer_input = VecDeque::new();
        for _ in 0..n_buffer {
            writer_input.push_back(BufferEmpty {
                buffer: vec![0; buffer_size].into_boxed_slice(),
            });
        }

        BufferWriter::Host(Box::new(Writer {
            current: None,
            state: Arc::new(Mutex::new(State {
                writer_input,
                reader_input: VecDeque::new(),
            })),
            item_size,
            reserved_items,
            reader_inbox: None,
            reader_input_id: None,
            writer_inbox,
            writer_output_id,
            finished: false,
        }))
    }
}

#[async_trait]
impl BufferWriterHost for Writer {
    fn add_reader(
        &mut self,
        reader_inbox: Sender<BlockMessage>,
        reader_input_id: usize,
    ) -> BufferReader {
        debug_assert!(self.reader_inbox.is_none());
        debug_assert!(self.reader_input_id.is_none());

        self.reader_inbox = Some(reader_inbox.clone());
        self.reader_input_id = Some(reader_input_id);

        BufferReader::Host(Box::new(Reader {
            current: None,
            state: self.state.clone(),
            item_size: self.item_size,
            reader_inbox,
            reserved_items: self.reserved_items,
            writer_inbox: self.writer_inbox.clone(),
            writer_output_id: self.writer_output_id,
            finished: false,
        }))
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn bytes(&mut self) -> (*mut u8, usize) {
        if self.current.is_none() {
            let mut state = self.state.lock().unwrap();
            if let Some(b) = state.writer_input.pop_front() {
                let capacity = b.buffer.len() / self.item_size;
                self.current = Some(CurrentBuffer {
                    buffer: b.buffer,
                    offset: self.reserved_items,
                    capacity,
                    tags: Vec::new(),
                });
            } else {
                return (std::ptr::null_mut::<u8>(), 0);
            }
        }

        let c = self.current.as_mut().unwrap();

        unsafe {
            (
                (c.buffer.as_mut_ptr() as *mut u8).add(c.offset * self.item_size),
                (c.capacity - c.offset) * self.item_size,
            )
        }
    }

    fn produce(&mut self, amount: usize, mut tags: Vec<ItemTag>) {
        debug_assert!(amount > 0);

        let c = self.current.as_mut().unwrap();
        debug_assert!(amount <= c.capacity - c.offset);
        for t in tags.iter_mut() {
            t.index += c.offset;
        }
        c.tags.append(&mut tags);
        c.offset += amount;
        if c.offset == c.capacity {
            let c = self.current.take().unwrap();
            let mut state = self.state.lock().unwrap();

            state.reader_input.push_back(BufferFull {
                buffer: c.buffer,
                items: c.capacity - self.reserved_items,
                tags: c.tags,
            });

            let _ = self
                .reader_inbox
                .as_mut()
                .unwrap()
                .try_send(BlockMessage::Notify);

            // make sure to be called again, if we have another buffer queued
            if !state.writer_input.is_empty() {
                let _ = self.writer_inbox.try_send(BlockMessage::Notify);
            }
        }
    }

    async fn notify_finished(&mut self) {
        if self.finished {
            return;
        }

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
            .as_mut()
            .unwrap()
            .send(BlockMessage::StreamInputDone {
                input_id: self.reader_input_id.unwrap(),
            })
            .await;
    }

    fn finish(&mut self) {
        self.finished = true;
    }

    fn finished(&self) -> bool {
        self.finished
    }
}

unsafe impl Send for Writer {}

#[derive(Debug)]
pub struct Reader {
    current: Option<CurrentBuffer>,
    state: Arc<Mutex<State>>,
    item_size: usize,
    reserved_items: usize,
    reader_inbox: Sender<BlockMessage>,
    writer_inbox: Sender<BlockMessage>,
    writer_output_id: usize,
    finished: bool,
}

#[async_trait]
impl BufferReaderHost for Reader {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn bytes(&mut self) -> (*const u8, usize, Vec<ItemTag>) {
        if let Some(cur) = self.current.as_mut() {
            let left = cur.capacity - cur.offset;
            debug_assert!(left > 0);
            if left <= self.reserved_items {
                let mut state = self.state.lock().unwrap();
                if let Some(BufferFull {
                    mut buffer,
                    mut tags,
                    items,
                }) = state.reader_input.pop_front()
                {
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            cur.buffer.as_ptr().add(cur.offset * self.item_size),
                            buffer
                                .as_mut_ptr()
                                .add((self.reserved_items - left) * self.item_size),
                            left * self.item_size,
                        );
                    }

                    for t in tags.iter_mut() {
                        t.index += left;
                    }
                    cur.tags.append(&mut tags);

                    let old = std::mem::replace(&mut cur.buffer, buffer);
                    state.writer_input.push_back(BufferEmpty { buffer: old });
                    let _ = self.writer_inbox.try_send(BlockMessage::Notify);

                    cur.capacity = items + self.reserved_items;
                    cur.offset = self.reserved_items - left;
                }
            }
        } else {
            let mut state = self.state.lock().unwrap();
            if let Some(b) = state.reader_input.pop_front() {
                let capacity = b.items + self.reserved_items;
                self.current = Some(CurrentBuffer {
                    buffer: b.buffer,
                    offset: self.reserved_items,
                    capacity,
                    tags: b.tags,
                });
            } else {
                return (std::ptr::null::<u8>(), 0, Vec::new());
            }
        }

        let c = self.current.as_mut().unwrap();

        unsafe {
            (
                (c.buffer.as_ptr() as *const u8).add(c.offset * self.item_size),
                (c.capacity - c.offset) * self.item_size,
                c.tags.clone(),
            )
        }
    }

    fn consume(&mut self, amount: usize) {
        debug_assert!(amount > 0);

        let c = self.current.as_mut().unwrap();
        debug_assert!(amount <= c.capacity - c.offset);
        c.offset += amount;

        if c.offset == c.capacity {
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
        } else if c.capacity - c.offset <= self.reserved_items {
            let state = self.state.lock().unwrap();
            if !state.reader_input.is_empty() {
                let _ = self.reader_inbox.try_send(BlockMessage::Notify);
            }
        }
    }

    async fn notify_finished(&mut self) {
        if self.finished {
            return;
        }

        let _ = self
            .writer_inbox
            .send(BlockMessage::StreamOutputDone {
                output_id: self.writer_output_id,
            })
            .await;
    }

    fn finish(&mut self) {
        self.finished = true;
    }

    fn finished(&self) -> bool {
        self.finished && self.state.lock().unwrap().reader_input.is_empty()
    }
}

unsafe impl Send for Reader {}
