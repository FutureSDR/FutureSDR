use std::env;
use std::time::Duration;
use std::time::Instant;

use futuresdr::blocks::MessageCopy;
use futuresdr::blocks::MessageSink;
use futuresdr::prelude::*;
use futuresdr::runtime::config;
use futuresdr::runtime::dev::prelude::*;
use futuresdr::runtime::scheduler::SmolScheduler;

#[derive(Debug, Clone, Copy)]
struct Args {
    messages: u64,
    stages: usize,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            messages: 100_000,
            stages: 4,
        }
    }
}

impl Args {
    fn parse() -> Result<Option<Self>> {
        let mut args = Self::default();
        let mut iter = env::args().skip(1);

        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "-h" | "--help" => return Ok(None),
                "-m" | "--messages" => {
                    args.messages = parse_next(&mut iter, "--messages")?;
                }
                "-s" | "--stages" => {
                    args.stages = parse_next(&mut iter, "--stages")?;
                }
                _ if arg.starts_with("--messages=") => {
                    args.messages = parse_value(&arg["--messages=".len()..], "--messages")?;
                }
                _ if arg.starts_with("--stages=") => {
                    args.stages = parse_value(&arg["--stages=".len()..], "--stages")?;
                }
                _ => {
                    return Err(Error::RuntimeError(format!(
                        "unknown argument {arg:?}; use --help"
                    ))
                    .into());
                }
            }
        }

        Ok(Some(args))
    }

    fn print_usage(program: &str) {
        println!(
            "Usage: {program} [--messages N] [--stages N]\n\n\
             Builds msg src -> msg copy x stages -> msg sink and runs it twice:\n\
               1. normal blocks on SmolScheduler::new(1, false)\n\
               2. all blocks inside one local domain\n\n\
             Defaults: --messages 100000 --stages 4"
        );
    }
}

fn parse_next<T: std::str::FromStr>(
    iter: &mut impl Iterator<Item = String>,
    name: &str,
) -> Result<T> {
    let value = iter
        .next()
        .ok_or_else(|| Error::RuntimeError(format!("missing value for {name}")))?;
    parse_value(&value, name)
}

fn parse_value<T: std::str::FromStr>(value: &str, name: &str) -> Result<T> {
    value
        .parse()
        .map_err(|_| Error::RuntimeError(format!("invalid value for {name}: {value:?}")).into())
}

#[derive(Block)]
#[message_outputs(out)]
struct MessageBenchSource {
    messages: u64,
}

impl MessageBenchSource {
    fn new(messages: u64) -> Self {
        Self { messages }
    }
}

impl Kernel for MessageBenchSource {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        mo: &mut MessageOutputs,
        _meta: &BlockMeta,
    ) -> Result<()> {
        for i in 0..self.messages {
            mo.post("out", Pmt::U64(i)).await?;
        }
        io.finished = true;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
enum Placement {
    NormalSmolOneThread,
    LocalDomain,
}

impl Placement {
    fn label(self) -> &'static str {
        match self {
            Self::NormalSmolOneThread => "smol-1-thread",
            Self::LocalDomain => "local-domain",
        }
    }
}

#[derive(Debug)]
struct RunStats {
    placement: Placement,
    elapsed: Duration,
    received: u64,
}

fn build_flowgraph(
    placement: Placement,
    messages: u64,
    stages: usize,
) -> Result<(Flowgraph, BlockRef<MessageSink>)> {
    let mut fg = Flowgraph::new();

    match placement {
        Placement::NormalSmolOneThread => {
            let src = fg.add(MessageBenchSource::new(messages))?;
            let mut prev = src.id();
            for _ in 0..stages {
                let copy = fg.add(MessageCopy::new())?;
                fg.message(prev, "out", copy.id(), "in")?;
                prev = copy.id();
            }
            let sink = fg.add(MessageSink::new())?;
            fg.message(prev, "out", sink.id(), "in")?;
            Ok((fg, sink))
        }
        Placement::LocalDomain => {
            let local = fg.local_domain()?;
            let sink = fg.with_local_domain(local, move |ctx| {
                let src = ctx.add(MessageBenchSource::new(messages));
                let mut prev = src.id();
                for _ in 0..stages {
                    let copy = ctx.add(MessageCopy::new());
                    ctx.message(prev, "out", copy.id(), "in")?;
                    prev = copy.id();
                }
                let sink = ctx.add(MessageSink::new());
                ctx.message(prev, "out", sink.id(), "in")?;
                Ok(sink)
            })?;
            Ok((fg, sink))
        }
    }
}

fn run_once(placement: Placement, args: Args) -> Result<RunStats> {
    let (fg, sink) = build_flowgraph(placement, args.messages, args.stages)?;
    let rt = Runtime::with_scheduler(SmolScheduler::new(1, false));

    let t0 = Instant::now();
    let fg = rt.run(fg)?;
    let elapsed = t0.elapsed();
    let received = fg.with(&sink, |sink| sink.received())?;

    if received != args.messages {
        return Err(Error::RuntimeError(format!(
            "{} received {received} messages, expected {}",
            placement.label(),
            args.messages
        ))
        .into());
    }

    Ok(RunStats {
        placement,
        elapsed,
        received,
    })
}

fn print_stats(stats: &RunStats, stages: usize) {
    let messages = stats.received as f64;
    let hops = messages * (stages as f64 + 1.0);
    let seconds = stats.elapsed.as_secs_f64();
    let messages_per_second = messages / seconds;
    let ns_per_message = seconds * 1e9 / messages;
    let ns_per_hop = seconds * 1e9 / hops;

    println!(
        "{:<16} {:>10.3} ms  {:>12.0} msg/s  {:>9.1} ns/msg  {:>9.1} ns/hop",
        stats.placement.label(),
        seconds * 1e3,
        messages_per_second,
        ns_per_message,
        ns_per_hop,
    );
}

fn main() -> Result<()> {
    let program = env::args()
        .next()
        .unwrap_or_else(|| "message-passing-overhead".to_string());
    let Some(args) = Args::parse()? else {
        Args::print_usage(&program);
        return Ok(());
    };
    if args.messages == 0 {
        return Err(Error::RuntimeError("--messages must be greater than zero".to_string()).into());
    }

    config::set("ctrlport_enable", false);
    config::set("log_level", "warn");

    println!(
        "messages={} stages={} queue_size={} (unchanged)",
        args.messages,
        args.stages,
        config::config().queue_size
    );
    println!(
        "{:<16} {:>14}  {:>16}  {:>12}  {:>12}",
        "case", "elapsed", "throughput", "per msg", "per hop"
    );

    let normal = run_once(Placement::NormalSmolOneThread, args)?;
    print_stats(&normal, args.stages);

    let local = run_once(Placement::LocalDomain, args)?;
    print_stats(&local, args.stages);

    println!(
        "local / smol elapsed ratio: {:.2}x",
        local.elapsed.as_secs_f64() / normal.elapsed.as_secs_f64()
    );

    Ok(())
}
