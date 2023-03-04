use futures::channel::mpsc::Sender;
use std::any::Any;
use std::fmt::Debug;

use crate::runtime::buffer::BufferReaderHost;
use crate::runtime::buffer::BufferWriterHost;
use crate::runtime::BlockMessage;
use crate::runtime::BufferReader;
use crate::runtime::BufferWriter;
use crate::runtime::ItemTag;
use crate::runtime::Kernel;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Mocker for a block
///
/// A harness to run a block without a runtime. Used for unit tests and benchmarking.
pub struct Mocker<K> {
    block: TypedBlock<K>,
}

impl<K: Kernel + 'static> Mocker<K> {
    /// Create mocker
    pub fn new(block: TypedBlock<K>) -> Self {
        Mocker { block }
    }

    /// Add input buffer with given data
    pub fn input<T>(&mut self, id: usize, data: Vec<T>)
    where
        T: Debug + Send + 'static,
    {
        self.block
            .sio
            .input(id)
            .set_reader(BufferReader::Host(Box::new(MockReader::new(
                data,
                Vec::new(),
            ))));
    }

    /// Add input buffer with given data and tags
    pub fn input_with_tags<T>(&mut self, id: usize, data: Vec<T>, tags: Vec<ItemTag>)
    where
        T: Debug + Send + 'static,
    {
        self.block
            .sio
            .input(id)
            .set_reader(BufferReader::Host(Box::new(MockReader::new(data, tags))));
    }

    /// Initialize output buffer with given size
    pub fn init_output<T>(&mut self, id: usize, size: usize)
    where
        T: Debug + Send + 'static,
    {
        self.block
            .sio
            .output(id)
            .init(BufferWriter::Host(Box::new(MockWriter::<T>::new(size))));
    }

    /// Get data from output buffer
    pub fn output<T>(&mut self, id: usize) -> Vec<T>
    where
        T: Debug + Send + 'static,
    {
        let w = self.block.sio.output(id).writer_mut();
        if let BufferWriter::Host(w) = w {
            w.as_any().downcast_mut::<MockWriter<T>>().unwrap().get()
        } else {
            panic!("mocker: wrong output buffer (expected CPU, got Custom)");
        }
    }

    /// Run the mocker
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run(&mut self) {
        crate::async_io::block_on(self.run_async());
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
    index: usize,
    tags: Vec<ItemTag>,
}

impl<T: Debug + Send + 'static> MockReader<T> {
    pub fn new(data: Vec<T>, tags: Vec<ItemTag>) -> Self {
        MockReader {
            data,
            index: 0,
            tags,
        }
    }
}

#[async_trait]
impl<T: Debug + Send + 'static> BufferReaderHost for MockReader<T> {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
    fn bytes(&mut self) -> (*const u8, usize, Vec<ItemTag>) {
        unsafe {
            (
                self.data.as_ptr().add(self.index) as *const u8,
                (self.data.len() - self.index) * std::mem::size_of::<T>(),
                self.tags.clone(),
            )
        }
    }
    fn consume(&mut self, amount: usize) {
        self.index += amount;
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
struct MockWriter<T: Debug + Send + 'static> {
    data: Vec<T>,
}

impl<T: Debug + Send + 'static> MockWriter<T> {
    pub fn new(size: usize) -> Self {
        MockWriter::<T> {
            data: Vec::with_capacity(size),
        }
    }

    pub fn get(&mut self) -> Vec<T> {
        std::mem::take(&mut self.data)
    }
}

#[async_trait]
impl<T: Debug + Send + 'static> BufferWriterHost for MockWriter<T> {
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

    fn produce(&mut self, amount: usize, _tags: Vec<ItemTag>) {
        unsafe {
            self.data.set_len(self.data.len() + amount);
        }
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
