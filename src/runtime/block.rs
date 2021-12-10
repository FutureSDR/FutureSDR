use anyhow::Result;
use std::any::Any;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

use crate::runtime::BlockMeta;
use crate::runtime::MessageInput;
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

// TIP: Why not combine this with AsyncKernelT below? -wspeirs
#[async_trait]
pub trait AsyncKernel: Send {
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
pub trait SyncKernel: Send {
    fn work(
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

// TIP: I would consider breaking this down into individual traits: Block, Meta, Kernel, etc. If you want a single
// trait that combines them all, you can do this with: AsyncBlockT: Block + Meta + Kernel ... You don't even need
// to specify any methods in the top-level trait. -wspeirs
#[async_trait]
pub trait AsyncBlockT: Send + Any {
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
    fn message_input_is_async(&self, id: usize) -> bool;
    fn message_input_name_to_id(&self, name: &str) -> Option<usize>;
    fn message_outputs(&self) -> &Vec<MessageOutput>;
    fn message_outputs_mut(&mut self) -> &mut Vec<MessageOutput>;
    fn message_output(&self, id: usize) -> &MessageOutput;
    fn message_output_mut(&mut self, id: usize) -> &mut MessageOutput;
    fn message_output_name_to_id(&self, name: &str) -> Option<usize>;

    fn call_sync_handler(&mut self, id: usize, p: Pmt) -> Result<Pmt>;
    async fn call_async_handler(&mut self, id: usize, p: Pmt) -> Result<Pmt>;
    async fn post(&mut self, id: usize, p: Pmt);
}

#[async_trait]
pub trait SyncBlockT: Send + Any {
    // ##### BLOCK
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    // ##### META
    fn instance_name(&self) -> Option<&str>;
    fn set_instance_name(&mut self, name: &str);
    fn type_name(&self) -> &str;
    fn is_blocking(&self) -> bool;

    // ##### KERNEL
    fn work(&mut self, io: &mut WorkIo) -> Result<()>;
    async fn init(&mut self) -> Result<()>;
    async fn deinit(&mut self) -> Result<()>;

    // ##### STREAM IO
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
    fn message_input_is_async(&self, id: usize) -> bool;
    fn message_input_name_to_id(&self, name: &str) -> Option<usize>;
    fn message_outputs(&self) -> &Vec<MessageOutput>;
    fn message_outputs_mut(&mut self) -> &mut Vec<MessageOutput>;
    fn message_output(&self, id: usize) -> &MessageOutput;
    fn message_output_mut(&mut self, id: usize) -> &mut MessageOutput;
    fn message_output_name_to_id(&self, name: &str) -> Option<usize>;

    fn call_sync_handler(&mut self, id: usize, p: Pmt) -> Result<Pmt>;
    async fn call_async_handler(&mut self, id: usize, p: Pmt) -> Result<Pmt>;
    async fn post(&mut self, id: usize, p: Pmt);
}

pub struct AsyncBlock<T: AsyncKernel + Send + 'static> {
    meta: BlockMeta,
    sio: StreamIo,
    mio: MessageIo<T>,
    kernel: T,
}

#[async_trait]
impl<T: AsyncKernel + Send> AsyncBlockT for AsyncBlock<T> {
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
    fn message_input_is_async(&self, id: usize) -> bool {
        self.mio.input_is_async(id)
    }
    fn message_input_name_to_id(&self, name: &str) -> Option<usize> {
        self.mio.input_name_to_id(name)
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
    async fn call_async_handler(&mut self, id: usize, p: Pmt) -> Result<Pmt> {
        let h = match self.mio.input(id) {
            MessageInput::Async(t) => t.get_handler(),
            _ => panic!("message handler is not async!"),
        };
        let f = (h)(&mut self.kernel, &mut self.mio, &mut self.meta, p);
        f.await
    }
    fn call_sync_handler(&mut self, id: usize, p: Pmt) -> Result<Pmt> {
        let h = match self.mio.input(id) {
            MessageInput::Sync(t) => t.get_handler(),
            _ => panic!("message handler is not sync!"),
        };
        (h)(&mut self.kernel, &mut self.mio, &mut self.meta, p)
    }
    async fn post(&mut self, id: usize, p: Pmt) {
        self.mio.post(id, p).await;
    }
}

pub struct SyncBlock<T: SyncKernel + Send + 'static> {
    meta: BlockMeta,
    sio: StreamIo,
    mio: MessageIo<T>,
    kernel: T,
}

#[async_trait]
impl<T: SyncKernel + Send> SyncBlockT for SyncBlock<T> {
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
    fn work(&mut self, io: &mut WorkIo) -> Result<()> {
        self.kernel
            .work(io, &mut self.sio, &mut self.mio, &mut self.meta)
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
    fn message_input_is_async(&self, id: usize) -> bool {
        self.mio.input_is_async(id)
    }
    fn message_input_name_to_id(&self, name: &str) -> Option<usize> {
        self.mio.input_name_to_id(name)
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
    async fn call_async_handler(&mut self, id: usize, p: Pmt) -> Result<Pmt> {
        let h = match self.mio.input(id) {
            MessageInput::Async(t) => t.get_handler(),
            _ => panic!("message handler is not async!"),
        };
        let f = (h)(&mut self.kernel, &mut self.mio, &mut self.meta, p);
        f.await
    }
    fn call_sync_handler(&mut self, id: usize, p: Pmt) -> Result<Pmt> {
        let h = match self.mio.input(id) {
            MessageInput::Sync(t) => t.get_handler(),
            _ => panic!("message handler is not sync!"),
        };
        (h)(&mut self.kernel, &mut self.mio, &mut self.meta, p)
    }
    async fn post(&mut self, id: usize, p: Pmt) {
        self.mio.post(id, p).await;
    }
}

// TIP: Why did box these variants? You should be able to use generics here too, and avoid the vtable lookup. -wspeirs
#[derive(Debug)]
pub enum Block {
    Sync(Box<dyn SyncBlockT>),
    Async(Box<dyn AsyncBlockT>),
}

// TIP: I think you could avoid a lot of the new_(a)sync and as_(a)sync methods by simply having a Block trait that
// defines the common methods. Then structs for AsyncBlock and SyncBlock. Your connect (and other) functions will
// simply take a B: Block, and operate appropriately. If you _really_ needed to know the type of block (sync or async)
// you can always provide a type() method in the Block trait. -wspeirs
impl Block {
    pub fn new_async<T: AsyncKernel + Send + 'static>(
        meta: BlockMeta,
        sio: StreamIo,
        mio: MessageIo<T>,
        kernel: T,
    ) -> Block {
        Block::Async(Box::new(AsyncBlock {
            meta,
            sio,
            mio,
            kernel,
        }))
    }

    pub fn new_sync<T: SyncKernel + Send + 'static>(
        meta: BlockMeta,
        sio: StreamIo,
        mio: MessageIo<T>,
        kernel: T,
    ) -> Block {
        Block::Sync(Box::new(SyncBlock {
            meta,
            sio,
            mio,
            kernel,
        }))
    }

    pub fn as_async<T: AsyncKernel + Send + 'static>(&self) -> Option<&T> {
        match self {
            Block::Async(b) => b
                .as_any()
                .downcast_ref::<AsyncBlock<T>>()
                .map(|b| &b.kernel),
            _ => None,
        }
    }

    pub fn as_async_mut<T: AsyncKernel + Send + 'static>(&mut self) -> Option<&T> {
        match self {
            Block::Async(b) => b
                .as_any_mut()
                .downcast_mut::<AsyncBlock<T>>()
                .map(|b| &b.kernel),
            _ => None,
        }
    }

    pub fn as_sync<T: SyncKernel + Send + 'static>(&self) -> Option<&T> {
        match self {
            Block::Sync(b) => b.as_any().downcast_ref::<SyncBlock<T>>().map(|b| &b.kernel),
            _ => None,
        }
    }

    pub fn as_sync_mut<T: SyncKernel + Send + 'static>(&mut self) -> Option<&T> {
        match self {
            Block::Sync(b) => b
                .as_any_mut()
                .downcast_mut::<SyncBlock<T>>()
                .map(|b| &b.kernel),
            _ => None,
        }
    }

    // ##### BLOCK
    pub fn as_any(&self) -> &dyn Any {
        self
    }

    // ##### META
    pub fn instance_name(&self) -> Option<&str> {
        match self {
            Block::Sync(b) => b.instance_name(),
            Block::Async(b) => b.instance_name(),
        }
    }
    pub fn set_instance_name(&mut self, name: &str) {
        match self {
            Block::Sync(b) => b.set_instance_name(name),
            Block::Async(b) => b.set_instance_name(name),
        }
    }
    pub fn type_name(&self) -> &str {
        match self {
            Block::Sync(b) => b.type_name(),
            Block::Async(b) => b.type_name(),
        }
    }
    pub fn is_blocking(&self) -> bool {
        match self {
            Block::Sync(b) => b.is_blocking(),
            Block::Async(b) => b.is_blocking(),
        }
    }

    // ##### KERNEL
    pub async fn init(&mut self) -> Result<()> {
        match self {
            Block::Sync(b) => b.init().await,
            Block::Async(b) => b.init().await,
        }
    }
    pub async fn deinit(&mut self) -> Result<()> {
        match self {
            Block::Sync(b) => b.deinit().await,
            Block::Async(b) => b.deinit().await,
        }
    }

    // ##### STREAM IO
    pub fn stream_inputs(&self) -> &Vec<StreamInput> {
        match self {
            Block::Sync(b) => b.stream_inputs(),
            Block::Async(b) => b.stream_inputs(),
        }
    }
    pub fn stream_inputs_mut(&mut self) -> &mut Vec<StreamInput> {
        match self {
            Block::Sync(b) => b.stream_inputs_mut(),
            Block::Async(b) => b.stream_inputs_mut(),
        }
    }
    pub fn stream_input(&self, id: usize) -> &StreamInput {
        match self {
            Block::Sync(b) => b.stream_input(id),
            Block::Async(b) => b.stream_input(id),
        }
    }
    pub fn stream_input_mut(&mut self, id: usize) -> &mut StreamInput {
        match self {
            Block::Sync(b) => b.stream_input_mut(id),
            Block::Async(b) => b.stream_input_mut(id),
        }
    }
    pub fn stream_input_name_to_id(&self, name: &str) -> Option<usize> {
        match self {
            Block::Sync(b) => b.stream_input_name_to_id(name),
            Block::Async(b) => b.stream_input_name_to_id(name),
        }
    }
    pub fn stream_outputs(&self) -> &Vec<StreamOutput> {
        match self {
            Block::Sync(b) => b.stream_outputs(),
            Block::Async(b) => b.stream_outputs(),
        }
    }
    pub fn stream_outputs_mut(&mut self) -> &mut Vec<StreamOutput> {
        match self {
            Block::Sync(b) => b.stream_outputs_mut(),
            Block::Async(b) => b.stream_outputs_mut(),
        }
    }
    pub fn stream_output(&self, id: usize) -> &StreamOutput {
        match self {
            Block::Sync(b) => b.stream_output(id),
            Block::Async(b) => b.stream_output(id),
        }
    }
    pub fn stream_output_mut(&mut self, id: usize) -> &mut StreamOutput {
        match self {
            Block::Sync(b) => b.stream_output_mut(id),
            Block::Async(b) => b.stream_output_mut(id),
        }
    }
    pub fn stream_output_name_to_id(&self, name: &str) -> Option<usize> {
        match self {
            Block::Sync(b) => b.stream_output_name_to_id(name),
            Block::Async(b) => b.stream_output_name_to_id(name),
        }
    }

    // ##### MESSAGE IO
    pub fn message_input_is_async(&self, id: usize) -> bool {
        match self {
            Block::Sync(b) => b.message_input_is_async(id),
            Block::Async(b) => b.message_input_is_async(id),
        }
    }
    pub fn message_input_name_to_id(&self, name: &str) -> Option<usize> {
        match self {
            Block::Sync(b) => b.message_input_name_to_id(name),
            Block::Async(b) => b.message_input_name_to_id(name),
        }
    }
    pub fn message_outputs(&self) -> &Vec<MessageOutput> {
        match self {
            Block::Sync(b) => b.message_outputs(),
            Block::Async(b) => b.message_outputs(),
        }
    }
    pub fn message_outputs_mut(&mut self) -> &mut Vec<MessageOutput> {
        match self {
            Block::Sync(b) => b.message_outputs_mut(),
            Block::Async(b) => b.message_outputs_mut(),
        }
    }
    pub fn message_output(&self, id: usize) -> &MessageOutput {
        match self {
            Block::Sync(b) => b.message_output(id),
            Block::Async(b) => b.message_output(id),
        }
    }
    pub fn message_output_mut(&mut self, id: usize) -> &mut MessageOutput {
        match self {
            Block::Sync(b) => b.message_output_mut(id),
            Block::Async(b) => b.message_output_mut(id),
        }
    }
    pub fn message_output_name_to_id(&self, name: &str) -> Option<usize> {
        match self {
            Block::Sync(b) => b.message_output_name_to_id(name),
            Block::Async(b) => b.message_output_name_to_id(name),
        }
    }
    pub fn call_sync_handler(&mut self, id: usize, p: Pmt) -> Result<Pmt> {
        match self {
            Block::Sync(b) => b.call_sync_handler(id, p),
            Block::Async(b) => b.call_sync_handler(id, p),
        }
    }
    pub async fn call_async_handler(&mut self, id: usize, p: Pmt) -> Result<Pmt> {
        match self {
            Block::Sync(b) => b.call_async_handler(id, p).await,
            Block::Async(b) => b.call_async_handler(id, p).await,
        }
    }
    pub async fn post(&mut self, id: usize, p: Pmt) {
        match self {
            Block::Sync(b) => b.post(id, p).await,
            Block::Async(b) => b.post(id, p).await,
        }
    }
}

impl<T: AsyncKernel + Send> fmt::Debug for AsyncBlock<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AsyncBlock")
            .field("type_name", &self.type_name().to_string())
            .finish()
    }
}

impl<T: SyncKernel + Send> fmt::Debug for SyncBlock<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyncBlock")
            .field("type_name", &self.type_name().to_string())
            .finish()
    }
}

impl fmt::Debug for dyn AsyncBlockT {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AsyncBlock")
            .field("type_name", &self.type_name().to_string())
            .finish()
    }
}

impl fmt::Debug for dyn SyncBlockT {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyncBlock")
            .field("type_name", &self.type_name().to_string())
            .finish()
    }
}
