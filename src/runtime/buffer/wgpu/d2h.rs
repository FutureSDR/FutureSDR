use futures::channel::mpsc::Sender;
use futures::prelude::*;
use std::any::Any;
use std::sync::{Arc, Mutex};

use crate::runtime::buffer::wgpu::{GPUBufferEmpty, GPUBufferFull};
use crate::runtime::buffer::BufferBuilder;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferReaderHost;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::BufferWriterCustom;
use crate::runtime::AsyncMessage;

#[derive(Debug, PartialEq, Hash)]
pub struct D2H;

impl Eq for D2H {}

impl D2H {
    pub fn new() -> D2H {
        D2H
    }
}

impl Default for D2H {
    fn default() -> Self {
        Self::new()
    }
}

impl BufferBuilder for D2H {
    fn build(
        &self,
        item_size: usize,
        writer_inbox: Sender<AsyncMessage>,
        writer_output_id: usize,
    ) -> BufferWriter {
        WriterD2H::new(item_size, writer_inbox, writer_output_id)
    }
}

#[derive(Debug)]
pub struct WriterD2H {
    item_size: usize,
    inbound: Arc<Mutex<Vec<GPUBufferEmpty>>>,
    outbound: Arc<Mutex<Vec<GPUBufferFull>>>,
    finished: bool,
    writer_inbox: Sender<AsyncMessage>,
    writer_output_id: usize,
    reader_inbox: Option<Sender<AsyncMessage>>,
    reader_input_id: Option<usize>,
}

#[async_trait]
impl BufferWriterCustom for WriterD2H {
    fn add_reader(
        &mut self,
        reader_inbox: Sender<AsyncMessage>,
        reader_input_id: usize,
    ) -> BufferReader {
        debug_assert!(self.reader_inbox.is_none());
        debug_assert!(self.reader_input_id.is_none());

        self.reader_inbox = Some(reader_inbox);
        self.reader_input_id = Some(reader_input_id);

        BufferReader::Host(Box::new(ReaderD2H {
            buffer: None,
            outbound: self.inbound.clone(),
            inbound: self.outbound.clone(),
            item_size: self.item_size,
            writer_inbox: self.writer_inbox.clone(),
            writer_output_id: self.writer_output_id,
            finished: false,
        }))
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    async fn notify_finished(&mut self) {
        if self.finished {
            return;
        }

        self.reader_inbox
            .as_mut()
            .unwrap()
            .send(AsyncMessage::StreamInputDone {
                input_id: self.reader_input_id.unwrap(),
            })
            .await
            .unwrap();
    }

    fn finish(&mut self) {
        self.finished = true;
    }

    fn finished(&self) -> bool {
        self.finished
    }
}

impl WriterD2H {
    pub fn new(
        item_size: usize,
        writer_inbox: Sender<AsyncMessage>,
        writer_output_id: usize,
    ) -> BufferWriter {
        BufferWriter::Custom(Box::new(WriterD2H {
            item_size,
            outbound: Arc::new(Mutex::new(Vec::new())),
            inbound: Arc::new(Mutex::new(Vec::new())),
            finished: false,
            writer_inbox,
            writer_output_id,
            reader_inbox: None,
            reader_input_id: None,
        }))
    }

    pub fn buffers(&mut self) -> Vec<GPUBufferEmpty> {
        let mut vec = self.inbound.lock().unwrap();
        std::mem::take(&mut vec)
    }

    pub fn submit(&mut self, buffer: GPUBufferFull) {
        self.outbound.lock().unwrap().push(buffer);
        let _ = self
            .reader_inbox
            .as_mut()
            .unwrap()
            .try_send(AsyncMessage::Notify);
    }
}

unsafe impl Send for WriterD2H {}

#[derive(Debug)]
pub struct ReaderD2H {
    buffer: Option<CurrentBuffer>,
    inbound: Arc<Mutex<Vec<GPUBufferFull>>>,
    outbound: Arc<Mutex<Vec<GPUBufferEmpty>>>,
    item_size: usize,
    writer_inbox: Sender<AsyncMessage>,
    writer_output_id: usize,
    finished: bool,
}

#[derive(Debug)]
struct CurrentBuffer {
    buffer: GPUBufferFull,
    offset: usize,
}

#[async_trait]
impl BufferReaderHost for ReaderD2H {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
   // #[cfg(target_arch = "wasm32")]
    fn bytes(&mut self) -> (*const u8, usize) {
        debug!("D2H reader bytes");
        if self.buffer.is_none() {
            if let Some(b) = self.inbound.lock().unwrap().pop() {
                debug!("set gpuBuffer full from inbound");
                self.buffer = Some(CurrentBuffer {
                    buffer: b,
                    offset: 0,
                });
            } else {
                debug!("set wrong pointer");
                return (std::ptr::null::<u8>(), 0);
            }
        }

        unsafe {
            let buffer = self.buffer.as_ref().unwrap();
            let capacity = buffer.buffer.used_bytes / self.item_size;
          //  let ret = buffer.buffer.buffer.as_ptr(); // get_mapped_range().as_ptr();
            //let ret = var.as_mut_ptr();
            let range = buffer.buffer.buffer.slice(..).get_mapped_range();
            let ptr = range.as_ptr(); // get_mapped_range().as_ptr();
            //assert_eq!(range.len(), buffer.buffer.used_bytes);
           // let ptr = range.as_ptr();
            //drop(var);
           // buffer.buffer.buffer.unmap();
           // log::info!("Return Pointer:  {:?} ", ret);
            //log::info!("Ret Add - Start Address:  {:?}, ***,  Size: {:?} ", ret.add(buffer.offset * self.item_size),
             //   (capacity - buffer.offset) * self.item_size);

            (
                ptr.add(buffer.offset * self.item_size),
                (capacity - buffer.offset) * self.item_size,
            )
        }
    }
/*
    #[cfg(not(target_arch = "wasm32"))]
    fn bytes(&mut self) -> (*const u8, usize) {
        debug!("D2H reader bytes");
        if self.buffer.is_none() {
            if let Some(b) = self.inbound.lock().unwrap().pop() {
                self.buffer = Some(CurrentBuffer {
                    buffer: b,
                    offset: 0,
                });
            } else {
                return (std::ptr::null::<u8>(), 0);
            }
        }

        unsafe {
            let buffer = self.buffer.as_ref().unwrap();
            let capacity = buffer.buffer.used_bytes / self.item_size;
            let mut var = buffer.buffer.buffer.slice(..).get_mapped_range_mut(); // get_mapped_range().as_ptr();
            let ret = var.as_mut_ptr();
            //   let ret = buffer.buffer.buffer.slice(..).get_mapped_range().to_vec().as_ptr(); // get_mapped_range().as_ptr();
            //    (capacity - buffer.offset) * self.item_size);

            (
                ret.add(buffer.offset * self.item_size),
                (capacity - buffer.offset) * self.item_size,
            )
        }
    }

 */

    fn consume(&mut self, amount: usize) {
       // log::info!("D2H reader consume {} elements", amount);

        let buffer = self.buffer.as_mut().unwrap();
        let capacity =   buffer.buffer.used_bytes / self.item_size;
        log::info!("Consume -- capacity: {}, offset: {}", capacity, buffer.offset);
        debug_assert!(amount + buffer.offset <= capacity);
        debug_assert!(amount != 0);

        buffer.offset += amount;
        if buffer.offset == capacity {
            let buffer = self.buffer.take().unwrap().buffer.buffer;
            buffer.unmap();
            self.outbound.lock().unwrap().push(GPUBufferEmpty { buffer});

            if let Some(b) = self.inbound.lock().unwrap().pop() {
                self.buffer = Some(CurrentBuffer {
                    buffer: b,
                    offset: 0,
                });
            }

            let _ = self.writer_inbox.try_send(AsyncMessage::Notify);
        }
    }

    async fn notify_finished(&mut self) {
        debug!("D2H Reader finish");
        if self.finished {
            return;
        }

        self.writer_inbox
            .send(AsyncMessage::StreamOutputDone {
                output_id: self.writer_output_id,
            })
            .await
            .unwrap();
    }

    fn finish(&mut self) {
        self.finished = true;
    }

    fn finished(&self) -> bool {
        self.finished
    }
}

unsafe impl Send for ReaderD2H {}
