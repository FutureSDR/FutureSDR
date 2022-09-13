use std::any::Any;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

use crate::anyhow::Result;
use crate::runtime::BlockMeta;
use crate::runtime::MessageIo;
use crate::runtime::MessageOutput;
use crate::runtime::Pmt;
use crate::runtime::StreamInput;
use crate::runtime::StreamIo;
use crate::runtime::StreamOutput;

pub struct WorkIo {
    pub call_again: bool,
    pub finished: bool,
    pub block_on: Option<Pin<Box<dyn Future<Output = ()> + Send>>>,
}

impl WorkIo {
    pub fn block_on<F: Future<Output = ()> + Send + 'static>(&mut self, f: F) {
        self.block_on = Some(Box::pin(f));
    }
}

impl fmt::Debug for WorkIo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("WorkIo")
            .field("call_again", &self.call_again)
            .field("finished", &self.finished)
            .finish()
    }
}

#[async_trait]
pub trait Kernel: Send {
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        _s: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        Ok(())
    }
    async fn init(
        &mut self,
        _s: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        Ok(())
    }
    async fn deinit(
        &mut self,
        _s: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
pub trait BlockT: Send + Any {
    // ##### BLOCK
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    // ##### META
    fn instance_name(&self) -> Option<&str>;
    fn set_instance_name(&mut self, name: &str);
    fn type_name(&self) -> &str;
    fn is_blocking(&self) -> bool;

    // ##### KERNEL
    async fn work(&mut self, io: &mut WorkIo) -> Result<()>;
    async fn init(&mut self) -> Result<()>;
    async fn deinit(&mut self) -> Result<()>;

    // ##### STREAM IO
    fn commit(&mut self);
    #[allow(clippy::type_complexity)]
    fn set_tag_propagation(
        &mut self,
        f: Box<dyn FnMut(&mut [StreamInput], &mut [StreamOutput]) + Send + 'static>,
    );
    fn stream_inputs(&self) -> &Vec<StreamInput>;
    fn stream_inputs_mut(&mut self) -> &mut Vec<StreamInput>;
    fn stream_input(&self, id: usize) -> &StreamInput;
    fn stream_input_mut(&mut self, id: usize) -> &mut StreamInput;
    fn stream_input_name_to_id(&self, name: &str) -> Option<usize>;
    fn stream_outputs(&self) -> &Vec<StreamOutput>;
    fn stream_outputs_mut(&mut self) -> &mut Vec<StreamOutput>;
    fn stream_output(&self, id: usize) -> &StreamOutput;
    fn stream_output_mut(&mut self, id: usize) -> &mut StreamOutput;
    fn stream_output_name_to_id(&self, name: &str) -> Option<usize>;

    // ##### MESSAGE IO
    fn message_input_name_to_id(&self, name: &str) -> Option<usize>;
    fn message_input_names(&self) -> Vec<String>;
    fn message_outputs(&self) -> &Vec<MessageOutput>;
    fn message_outputs_mut(&mut self) -> &mut Vec<MessageOutput>;
    fn message_output(&self, id: usize) -> &MessageOutput;
    fn message_output_mut(&mut self, id: usize) -> &mut MessageOutput;
    fn message_output_name_to_id(&self, name: &str) -> Option<usize>;

    async fn call_handler(&mut self, id: usize, p: Pmt) -> Result<Pmt>;
    async fn post(&mut self, id: usize, p: Pmt);
}

pub struct TypedBlock<T> {
    meta: BlockMeta,
    sio: StreamIo,
    mio: MessageIo<T>,
    kernel: T,
}

#[async_trait]
impl<T: Kernel + Send + 'static> BlockT for TypedBlock<T> {
    // ##### Block
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    // ##### META
    fn instance_name(&self) -> Option<&str> {
        self.meta.instance_name()
    }
    fn set_instance_name(&mut self, name: &str) {
        self.meta.set_instance_name(name);
    }
    fn type_name(&self) -> &str {
        self.meta.type_name()
    }
    fn is_blocking(&self) -> bool {
        self.meta.is_blocking()
    }

    // ##### KERNEL
    async fn work(&mut self, io: &mut WorkIo) -> Result<()> {
        self.kernel
            .work(io, &mut self.sio, &mut self.mio, &mut self.meta)
            .await
    }
    async fn init(&mut self) -> Result<()> {
        self.kernel
            .init(&mut self.sio, &mut self.mio, &mut self.meta)
            .await
    }
    async fn deinit(&mut self) -> Result<()> {
        self.kernel
            .deinit(&mut self.sio, &mut self.mio, &mut self.meta)
            .await
    }

    // ##### STREAM IO
    fn commit(&mut self) {
        self.sio.commmit();
    }
    fn set_tag_propagation(
        &mut self,
        f: Box<dyn FnMut(&mut [StreamInput], &mut [StreamOutput]) + Send + 'static>,
    ) {
        self.sio.set_tag_propagation(f);
    }
    fn stream_inputs(&self) -> &Vec<StreamInput> {
        self.sio.inputs()
    }
    fn stream_inputs_mut(&mut self) -> &mut Vec<StreamInput> {
        self.sio.inputs_mut()
    }
    fn stream_input(&self, id: usize) -> &StreamInput {
        self.sio.input_ref(id)
    }
    fn stream_input_mut(&mut self, id: usize) -> &mut StreamInput {
        self.sio.input(id)
    }
    fn stream_input_name_to_id(&self, name: &str) -> Option<usize> {
        self.sio.input_name_to_id(name)
    }
    fn stream_outputs(&self) -> &Vec<StreamOutput> {
        self.sio.outputs()
    }
    fn stream_outputs_mut(&mut self) -> &mut Vec<StreamOutput> {
        self.sio.outputs_mut()
    }
    fn stream_output(&self, id: usize) -> &StreamOutput {
        self.sio.output_ref(id)
    }
    fn stream_output_mut(&mut self, id: usize) -> &mut StreamOutput {
        self.sio.output(id)
    }
    fn stream_output_name_to_id(&self, name: &str) -> Option<usize> {
        self.sio.output_name_to_id(name)
    }

    // ##### MESSAGE IO
    fn message_input_name_to_id(&self, name: &str) -> Option<usize> {
        self.mio.input_name_to_id(name)
    }
    fn message_input_names(&self) -> Vec<String> {
        self.mio.input_names()
    }
    fn message_outputs(&self) -> &Vec<MessageOutput> {
        self.mio.outputs()
    }
    fn message_outputs_mut(&mut self) -> &mut Vec<MessageOutput> {
        self.mio.outputs_mut()
    }
    fn message_output(&self, id: usize) -> &MessageOutput {
        self.mio.output(id)
    }
    fn message_output_mut(&mut self, id: usize) -> &mut MessageOutput {
        self.mio.output_mut(id)
    }
    fn message_output_name_to_id(&self, name: &str) -> Option<usize> {
        self.mio.output_name_to_id(name)
    }
    async fn call_handler(&mut self, id: usize, p: Pmt) -> Result<Pmt> {
        let h = self.mio.input(id).get_handler();
        let f = (h)(&mut self.kernel, &mut self.mio, &mut self.meta, p);
        f.await
    }
    async fn post(&mut self, id: usize, p: Pmt) {
        self.mio.post(id, p).await;
    }
}

#[derive(Debug)]
pub struct Block(Box<dyn BlockT>);

impl Block {
    pub fn new<T: Kernel + Send + 'static>(
        meta: BlockMeta,
        sio: StreamIo,
        mio: MessageIo<T>,
        kernel: T,
    ) -> Block {
        Self(Box::new(TypedBlock {
            meta,
            sio,
            mio,
            kernel,
        }))
    }

    pub fn kernel<T: Kernel + Send + 'static>(&self) -> Option<&T> {
        self.0
            .as_any()
            .downcast_ref::<TypedBlock<T>>()
            .map(|b| &b.kernel)
    }

    pub fn kernel_mut<T: Kernel + Send + 'static>(&mut self) -> Option<&T> {
        self.0
            .as_any_mut()
            .downcast_mut::<TypedBlock<T>>()
            .map(|b| &b.kernel)
    }

    // ##### META
    pub fn instance_name(&self) -> Option<&str> {
        self.0.instance_name()
    }
    pub fn set_instance_name(&mut self, name: impl AsRef<str>) {
        self.0.set_instance_name(name.as_ref())
    }
    pub fn type_name(&self) -> &str {
        self.0.type_name()
    }
    pub fn is_blocking(&self) -> bool {
        self.0.is_blocking()
    }

    // ##### KERNEL
    pub async fn init(&mut self) -> Result<()> {
        self.0.init().await
    }
    pub async fn work(&mut self, io: &mut WorkIo) -> Result<()> {
        self.0.work(io).await
    }
    pub async fn deinit(&mut self) -> Result<()> {
        self.0.deinit().await
    }

    // ##### STREAM IO
    pub fn commit(&mut self) {
        self.0.commit();
    }
    #[allow(clippy::type_complexity)]
    pub fn set_tag_propagation(
        &mut self,
        f: Box<dyn FnMut(&mut [StreamInput], &mut [StreamOutput]) + Send + 'static>,
    ) {
        self.0.set_tag_propagation(f);
    }
    pub fn stream_inputs(&self) -> &Vec<StreamInput> {
        self.0.stream_inputs()
    }
    pub fn stream_inputs_mut(&mut self) -> &mut Vec<StreamInput> {
        self.0.stream_inputs_mut()
    }
    pub fn stream_input(&self, id: usize) -> &StreamInput {
        self.0.stream_input(id)
    }
    pub fn stream_input_mut(&mut self, id: usize) -> &mut StreamInput {
        self.0.stream_input_mut(id)
    }
    pub fn stream_input_name_to_id(&self, name: &str) -> Option<usize> {
        self.0.stream_input_name_to_id(name)
    }
    pub fn stream_outputs(&self) -> &Vec<StreamOutput> {
        self.0.stream_outputs()
    }
    pub fn stream_outputs_mut(&mut self) -> &mut Vec<StreamOutput> {
        self.0.stream_outputs_mut()
    }
    pub fn stream_output(&self, id: usize) -> &StreamOutput {
        self.0.stream_output(id)
    }
    pub fn stream_output_mut(&mut self, id: usize) -> &mut StreamOutput {
        self.0.stream_output_mut(id)
    }
    pub fn stream_output_name_to_id(&self, name: &str) -> Option<usize> {
        self.0.stream_output_name_to_id(name)
    }

    // ##### MESSAGE IO
    pub fn message_input_name_to_id(&self, name: &str) -> Option<usize> {
        self.0.message_input_name_to_id(name)
    }
    pub fn message_input_names(&self) -> Vec<String> {
        self.0.message_input_names()
    }
    pub fn message_outputs(&self) -> &Vec<MessageOutput> {
        self.0.message_outputs()
    }
    pub fn message_outputs_mut(&mut self) -> &mut Vec<MessageOutput> {
        self.0.message_outputs_mut()
    }
    pub fn message_output(&self, id: usize) -> &MessageOutput {
        self.0.message_output(id)
    }
    pub fn message_output_mut(&mut self, id: usize) -> &mut MessageOutput {
        self.0.message_output_mut(id)
    }
    pub fn message_output_name_to_id(&self, name: &str) -> Option<usize> {
        self.0.message_output_name_to_id(name)
    }
    pub async fn call_handler(&mut self, id: usize, p: Pmt) -> Result<Pmt> {
        self.0.call_handler(id, p).await
    }
    pub async fn post(&mut self, id: usize, p: Pmt) {
        self.0.post(id, p).await
    }
}

impl<T: Kernel + Send + 'static> fmt::Debug for TypedBlock<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AsyncBlock")
            .field("type_name", &self.type_name().to_string())
            .finish()
    }
}

impl fmt::Debug for dyn BlockT {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlockT")
            .field("type_name", &self.type_name().to_string())
            .finish()
    }
}
