use std::future::Future;
use std::pin::Pin;

use crate::runtime::Result;
use crate::runtime::dev::BlockMeta;
use crate::runtime::dev::MessageOutputs;
use crate::runtime::dev::WorkIo;
use crate::runtime::kernel_interface::KernelInterface;

/// Send-capable marker for normal runtime blocks.
///
/// This keeps verbose return-type-notation bounds in one place: normal blocks
/// must have a `Send` value, `Send` block-on future, and `Send` futures returned
/// from the kernel lifecycle methods and generated block interface.
#[doc(hidden)]
pub trait SendKernel: Kernel<BlockOn: Send> + KernelInterface + Send
where
    Self: Kernel<work(..): Send, init(..): Send, deinit(..): Send>,
    Self: KernelInterface<stream_ports_notify_finished(..): Send, call_handler(..): Send>,
{
}

impl<T> SendKernel for T where
    T: Kernel<BlockOn: Send, work(..): Send, init(..): Send, deinit(..): Send>
        + KernelInterface<stream_ports_notify_finished(..): Send, call_handler(..): Send>
        + Send
{
}

/// Processing logic for a block.
///
/// `Kernel` is the central trait custom block authors implement. The
/// `#[derive(Block)]` macro declares stream and message ports from annotated
/// fields and methods; the `Kernel` implementation supplies initialization,
/// work, and shutdown behavior.
///
/// The runtime calls [`Kernel::init`] once, then repeatedly calls
/// [`Kernel::work`] until the block marks itself finished or the flowgraph is
/// stopped, and finally calls [`Kernel::deinit`]. A `work()` implementation
/// should consume and produce exactly the number of stream items it handled and
/// use [`WorkIo`] to request another immediate call or finish. Blocks that need
/// to wait on their own future can provide it through [`Kernel::block_on`].
///
/// Normal runtime entry points accept only kernels whose value, block-on future,
/// and returned futures are `Send`. Kernels that do not satisfy these bounds can
/// still run in a local domain.
///
/// ```
/// use futuresdr::runtime::dev::prelude::*;
///
/// #[derive(Block)]
/// struct Scale {
///     #[input]
///     input: DefaultCpuReader<f32>,
///     #[output]
///     output: DefaultCpuWriter<f32>,
///     gain: f32,
/// }
///
/// impl Kernel for Scale {
///     async fn work(
///         &mut self,
///         io: &mut WorkIo,
///         _mo: &mut MessageOutputs,
///         _meta: &BlockMeta,
///     ) -> Result<()> {
///         let input = self.input.slice();
///         let output = self.output.slice();
///         let n = input.len().min(output.len());
///
///         for i in 0..n {
///             output[i] = input[i] * self.gain;
///         }
///
///         self.input.consume(n);
///         self.output.produce(n);
///
///         if self.input.finished() {
///             io.finished = true;
///         }
///
///         Ok(())
///     }
/// }
/// ```
pub trait Kernel {
    /// Typed future that may be used to wake the block again.
    type BlockOn: Future<Output = ()> + 'static = std::future::Pending<()>;

    /// Return a typed future that may wake this block again.
    ///
    /// When `work()` does not request an immediate call and the block is not
    /// finished, the runtime waits for inbox/stream notifications. If this
    /// method returns `Some`, the runtime also awaits that future and calls
    /// `work()` again when either the future resolves or a notification arrives.
    /// Return `None` while no block-specific future should be polled.
    fn block_on(&mut self) -> Option<Pin<&mut Self::BlockOn>> {
        None
    }

    /// Process stream data and emit messages.
    ///
    /// Implementations inspect their input buffers, write output buffers, update
    /// consume/produce counts, optionally post PMTs through [`MessageOutputs`],
    /// and update [`WorkIo`] flags before returning.
    fn work(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _b: &BlockMeta,
    ) -> impl Future<Output = Result<()>> {
        async { Ok(()) }
    }

    /// Initialize the kernel before normal work starts.
    ///
    /// This is the place to allocate runtime resources or send initial messages.
    /// Stream ports have already been initialized and validated when this method
    /// is called. Runtime metadata is read-only here; mutable block state should
    /// live in the kernel implementation.
    fn init(
        &mut self,
        _mo: &mut MessageOutputs,
        _b: &BlockMeta,
    ) -> impl Future<Output = Result<()>> {
        async { Ok(()) }
    }

    /// De-initialize the kernel after work has stopped.
    ///
    /// This is called during block shutdown even when the block stopped because
    /// the flowgraph was terminated. It should release resources owned by the
    /// block and may post final messages.
    fn deinit(
        &mut self,
        _mo: &mut MessageOutputs,
        _b: &BlockMeta,
    ) -> impl Future<Output = Result<()>> {
        async { Ok(()) }
    }
}
