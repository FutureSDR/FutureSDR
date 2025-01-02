use clap::Parser;
use clap::ValueEnum;
use futuresdr::macros::connect;
use futuresdr::runtime::scheduler;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::Result;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::TypedBlock;
use futuresdr::runtime::WorkIo;

#[derive(Debug, Clone, ValueEnum)]
enum PanicWhere {
    Init,
    Work,
    Deinit,
}

#[derive(Debug, Clone, ValueEnum)]
enum Scheduler {
    Tpb,
    Smol,
    Flow,
}

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long, value_enum, default_value_t = PanicWhere::Work)]
    panic_where: PanicWhere,
    #[clap(short, long, value_enum, default_value_t = Scheduler::Smol)]
    scheduler: Scheduler,
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("Configuration: {args:?}");

    let mut fg = Flowgraph::new();
    let p = Panic::new(args.panic_where);

    connect!(fg, p);

    match args.scheduler {
        Scheduler::Tpb => {
            let _ = Runtime::with_scheduler(scheduler::TpbScheduler::new()).run(fg);
        }
        Scheduler::Smol => {
            let _ = Runtime::new().run(fg);
        }
        Scheduler::Flow => {
            let _ = Runtime::with_scheduler(scheduler::FlowScheduler::new()).run(fg);
        }
    }

    Ok(())
}

struct Panic {
    w: PanicWhere,
}

impl Panic {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(w: PanicWhere) -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("Panic").build(),
            StreamIoBuilder::new().build(),
            MessageIoBuilder::<Self>::new().build(),
            Self { w },
        )
    }
}

impl Kernel for Panic {
    async fn init(
        &mut self,
        _s: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        if matches!(self.w, PanicWhere::Init) {
            panic!("test panic");
        }
        Ok(())
    }
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _s: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        if matches!(self.w, PanicWhere::Work) {
            panic!("test panic");
        } else {
            io.finished = true;
        }
        Ok(())
    }
    async fn deinit(
        &mut self,
        _s: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        if matches!(self.w, PanicWhere::Deinit) {
            panic!("test panic");
        }
        Ok(())
    }
}
