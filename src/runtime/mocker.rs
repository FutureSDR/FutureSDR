use futures::channel::mpsc::Sender;
use std::any::Any;
use std::fmt::Debug;

use crate::runtime::buffer::BufferReaderHost;
use crate::runtime::buffer::BufferWriterHost;
use crate::runtime::AsyncMessage;
use crate::runtime::Block;
use crate::runtime::BufferReader;
use crate::runtime::BufferWriter;
use crate::runtime::WorkIo;

pub struct Mocker {
    block: Block,
}

impl Mocker {
    pub fn new(block: Block) -> Self {
        Mocker { block }
    }

    pub fn input<T>(&mut self, id: usize, data: Vec<T>)
    where
        T: Debug + Send + 'static,
    {
        self.block
            .stream_input_mut(id)
            .set_reader(BufferReader::Host(Box::new(MockReader::new(data))));
    }

    pub fn init_output<T>(&mut self, id: usize, size: usize)
    where
        T: Debug + Send + 'static,
    {
        self.block
            .stream_output_mut(id)
            .init(BufferWriter::Host(Box::new(MockWriter::<T>::new(size))));
    }

    pub fn output<T>(&mut self, id: usize) -> Vec<T>
    where
        T: Debug + Send + 'static,
    {
        let w = self.block.stream_output_mut(id).writer_mut();
        if let BufferWriter::Host(w) = w {
            w.as_any().downcast_mut::<MockWriter<T>>().unwrap().get()
        } else {
            panic!("mocker: wrong output buffer (expected CPU, got Custom)");
        }
    }

    pub fn run(&mut self) {
        let mut io = WorkIo {
            call_again: false,
            finished: false,
            block_on: None,
        };

        crate::async_io::block_on(async move {
            loop {
                self.block.work(&mut io).await.unwrap();
                if !io.call_again {
                    break;
                } else {
                    io.call_again = false;
                }
            }
        });
    }
}

#[derive(Debug)]
struct MockReader<T: Debug + Send + 'static> {
    data: Vec<T>,
    index: usize,
}

impl<T: Debug + Send + 'static> MockReader<T> {
    pub fn new(data: Vec<T>) -> Self {
        MockReader { data, index: 0 }
    }
}

#[async_trait]
impl<T: Debug + Send + 'static> BufferReaderHost for MockReader<T> {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
    fn bytes(&mut self) -> (*const u8, usize) {
        unsafe {
            (
                self.data.as_ptr().add(self.index) as *const u8,
                (self.data.len() - self.index) * std::mem::size_of::<T>(),
            )
        }
    }
    fn consume(&mut self, amount: usize) {
        self.index += amount;
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
        _reader_inbox: Sender<AsyncMessage>,
        _reader_input_id: usize,
    ) -> BufferReader {
        unimplemented!();
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn produce(&mut self, amount: usize) {
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
