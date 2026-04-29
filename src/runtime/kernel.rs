use std::future::Future;

use crate::runtime::dev::BlockMeta;
use crate::runtime::dev::MaybeSend;
use crate::runtime::dev::MessageOutputs;
use crate::runtime::dev::WorkIo;
use futuresdr::runtime::Result;

/// Processing logic for a block.
///
/// `Kernel` is the central trait custom block authors implement. The
/// `#[derive(Block)]` macro declares stream and message ports from annotated
/// fields and methods; the `Kernel` implementation supplies initialization,
/// work, and shutdown behavior.
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
///         _meta: &mut BlockMeta,
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
pub trait Kernel: MaybeSend {
    /// Process stream data and emit messages.
    ///
    /// The runtime calls this repeatedly while inputs, outputs, messages, or
    /// timers can make progress. Use [`WorkIo`] to request another immediate
    /// call, wait on a future, or mark the block as finished.
    fn work(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> impl Future<Output = Result<()>> + MaybeSend {
        async { Ok(()) }
    }
    /// Initialize the kernel before normal work starts.
    ///
    /// Override this to allocate resources, post startup messages, or adjust
    /// metadata. The default implementation does nothing.
    fn init(
        &mut self,
        _mo: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> impl Future<Output = Result<()>> + MaybeSend {
        async { Ok(()) }
    }
    /// De-initialize the kernel after work has stopped.
    ///
    /// Override this to release resources or post final messages. The default
    /// implementation does nothing.
    fn deinit(
        &mut self,
        _mo: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> impl Future<Output = Result<()>> + MaybeSend {
        async { Ok(()) }
    }
}
