use futures::channel::mpsc::channel;
use futures::channel::mpsc::Receiver;
use futures::channel::mpsc::Sender;
use std::any::Any;
use std::fmt::Debug;

use crate::runtime::buffer::BufferReaderHost;
use crate::runtime::buffer::BufferWriterHost;
use crate::runtime::config::config;
use crate::runtime::BlockMessage;
use crate::runtime::BlockPortCtx;
use crate::runtime::BufferReader;
use crate::runtime::BufferWriter;
use crate::runtime::Error;
use crate::runtime::ItemTag;
use crate::runtime::Kernel;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Mocker for a block
///
/// A harness to run a block without a runtime. Used for unit tests and benchmarking.
pub struct Mocker<K> {
    block: TypedBlock<K>,
    message_sinks: Vec<Receiver<BlockMessage>>,
    messages: Vec<Vec<Pmt>>,
}

impl<K: Kernel + 'static> Mocker<K> {
    /// Create mocker
    pub fn new(mut block: TypedBlock<K>) -> Self {
        let mut messages = Vec::new();
        let mut message_sinks = Vec::new();
        let msg_len = config().queue_size;
        for (n, p) in block.mio.outputs_mut().iter_mut().enumerate() {
            messages.push(Vec::new());
            let (tx, rx) = channel(msg_len);
            message_sinks.push(rx);
            p.connect(n, tx);
        }

        Mocker {
            block,
            message_sinks,
            messages,
        }
    }

    /// Add input buffer with given data
    pub fn input<T>(&mut self, id: usize, data: Vec<T>)
    where
        T: Debug + Send + 'static,
    {
        self.input_with_tags(id, data, Vec::new());
    }

    /// Add input buffer with given data and tags
    pub fn input_with_tags<T>(&mut self, id: usize, mut data: Vec<T>, mut tags: Vec<ItemTag>)
    where
        T: Debug + Send + 'static,
    {
        match self.block.sio.input(id).try_as::<MockReader<T>>() {
            Some(r) => {
                let offset = r.data.len();
                for t in tags.iter_mut() {
                    t.index += offset;
                }

                r.data.append(&mut data);
                r.tags.append(&mut tags);
            }
            _ => {
                self.block
                    .sio
                    .input(id)
                    .set_reader(BufferReader::Host(Box::new(MockReader::new(data, tags))));
            }
        }
    }

    /// Initialize output buffer with given size
    pub fn init_output<T>(&mut self, id: usize, size: usize)
    where
        T: Clone + Debug + Send + 'static,
    {
        self.block
            .sio
            .output(id)
            .init(BufferWriter::Host(Box::new(MockWriter::<T>::new(size))));
    }

    /// Post a PMT to a message handler of the block.
    pub fn post(&mut self, id: PortId, p: Pmt) -> Result<Pmt, Error> {
        let id = match id {
            PortId::Name(ref n) => self
                .block
                .mio
                .input_name_to_id(n)
                .ok_or(Error::InvalidMessagePort(BlockPortCtx::None, id))?,
            PortId::Index(id) => id,
        };

        let mut io = WorkIo {
            call_again: false,
            finished: false,
            block_on: None,
        };

        let TypedBlock {
            meta, mio, kernel, ..
        } = &mut self.block;
        let h = mio.input(id).get_handler();
        let f = (h)(kernel, &mut io, mio, meta, p);
        async_io::block_on(f).map_err(|e| Error::HandlerError(e.to_string()))
    }

    /// Get data from output buffer
    pub fn output<T>(&mut self, id: usize) -> (Vec<T>, Vec<ItemTag>)
    where
        T: Clone + Debug + Send + 'static,
    {
        let w = self.block.sio.output(id).writer_mut();
        if let BufferWriter::Host(w) = w {
            w.as_any().downcast_ref::<MockWriter<T>>().unwrap().get()
        } else {
            panic!("mocker: wrong output buffer (expected CPU, got Custom)");
        }
    }

    /// Taking data from output buffer, freeing up the buffer
    pub fn take_output<T>(&mut self, id: usize) -> (Vec<T>, Vec<ItemTag>)
    where
        T: Clone + Debug + Send + 'static,
    {
        let w = self.block.sio.output(id).writer_mut();
        if let BufferWriter::Host(w) = w {
            w.as_any().downcast_mut::<MockWriter<T>>().unwrap().take()
        } else {
            panic!("mocker: wrong output buffer (expected CPU, got Custom)");
        }
    }

    /// Run the block wrapped by the mocker
    pub fn run(&mut self) {
        crate::async_io::block_on(self.run_async());
    }

    /// Init the block wrapped by the mocker
    pub fn init(&mut self) {
        crate::async_io::block_on(async {
            self.block
                .kernel
                .init(
                    &mut self.block.sio,
                    &mut self.block.mio,
                    &mut self.block.meta,
                )
                .await
                .unwrap();
        });
    }

    /// Deinit the block wrapped by the mocker
    pub fn deinit(&mut self) {
        crate::async_io::block_on(async {
            self.block
                .kernel
                .deinit(
                    &mut self.block.sio,
                    &mut self.block.mio,
                    &mut self.block.meta,
                )
                .await
                .unwrap();
        });
    }

    /// Get produced PMTs from output message ports.
    pub fn messages(&self) -> Vec<Vec<Pmt>> {
        self.messages.clone()
    }

    /// Take produced PMTs from output message ports.
    pub fn take_messages(&mut self) -> Vec<Vec<Pmt>> {
        std::mem::take(&mut self.messages)
    }

    /// Run the mocker async
    pub async fn run_async(&mut self) {
        let mut io = WorkIo {
            call_again: false,
            finished: false,
            block_on: None,
        };

        loop {
            self.block
                .kernel
                .work(
                    &mut io,
                    &mut self.block.sio,
                    &mut self.block.mio,
                    &mut self.block.meta,
                )
                .await
                .unwrap();
            self.block.sio.commit();

            for (n, r) in self.message_sinks.iter_mut().enumerate() {
                while let Ok(Some(m)) = r.try_next() {
                    match m {
                        BlockMessage::Call { data, .. } => {
                            self.messages[n].push(data);
                        }
                        _ => panic!("Mocked Block produced unexpected BlockMessage {m:?}"),
                    }
                }
            }

            if !io.call_again {
                break;
            } else {
                io.call_again = false;
            }
        }
    }
}

#[derive(Debug)]
struct MockReader<T: Debug + Send + 'static> {
    data: Vec<T>,
    tags: Vec<ItemTag>,
}

impl<T: Debug + Send + 'static> MockReader<T> {
    pub fn new(data: Vec<T>, tags: Vec<ItemTag>) -> Self {
        MockReader { data, tags }
    }
}

#[async_trait]
impl<T: Debug + Send + 'static> BufferReaderHost for MockReader<T> {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
    fn bytes(&mut self) -> (*const u8, usize, Vec<ItemTag>) {
        (
            self.data.as_ptr() as *const u8,
            self.data.len() * std::mem::size_of::<T>(),
            self.tags.clone(),
        )
    }
    fn consume(&mut self, amount: usize) {
        self.data = self.data.split_off(amount);
        self.tags.retain(|x| x.index >= amount);

        for t in self.tags.iter_mut() {
            t.index -= amount;
        }
    }
    async fn notify_finished(&mut self) {}
    fn finish(&mut self) {}
    fn finished(&self) -> bool {
        true
    }
}

#[derive(Debug)]
struct MockWriter<T: Clone + Debug + Send + 'static> {
    data: Vec<T>,
    tags: Vec<ItemTag>,
}

impl<T: Clone + Debug + Send + 'static> MockWriter<T> {
    pub fn new(size: usize) -> Self {
        MockWriter::<T> {
            data: Vec::with_capacity(size),
            tags: Vec::new(),
        }
    }

    pub fn get(&self) -> (Vec<T>, Vec<ItemTag>) {
        (self.data.clone(), self.tags.clone())
    }

    pub fn take(&mut self) -> (Vec<T>, Vec<ItemTag>) {
        let (data, tags) = self.get();
        self.data.clear();
        self.tags = Vec::new();
        (data, tags)
    }
}

#[async_trait]
impl<T: Clone + Debug + Send + 'static> BufferWriterHost for MockWriter<T> {
    fn add_reader(
        &mut self,
        _reader_inbox: Sender<BlockMessage>,
        _reader_input_id: usize,
    ) -> BufferReader {
        unimplemented!();
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn produce(&mut self, amount: usize, tags: Vec<ItemTag>) {
        let curr_len = self.data.len();
        unsafe {
            self.data.set_len(curr_len + amount);
        }
        self.tags.extend(tags.into_iter().map(|mut t| {
            t.index += curr_len;
            t
        }));
    }

    fn bytes(&mut self) -> (*mut u8, usize) {
        unsafe {
            (
                self.data.as_mut_ptr().add(self.data.len()) as *mut u8,
                (self.data.capacity() - self.data.len()) * std::mem::size_of::<T>(),
            )
        }
    }

    async fn notify_finished(&mut self) {}
    fn finish(&mut self) {}
    fn finished(&self) -> bool {
        false
    }
}
