# Logging

FutureSDR uses [`tracing`](https://docs.rs/tracing/) for log and diagnostic messages. The common tracing macros are re-exported through the prelude, so application code can use them directly:

```rust
use futuresdr::prelude::*;

info!("starting application");
debug!("configured sample rate: {}", 1_000_000);
warn!("using fallback configuration");
```

The same macros are used internally by FutureSDR blocks, schedulers, and runtime code.

## Default Logger

If no global tracing subscriber has been installed when a runtime is constructed, FutureSDR installs its default logger. The default logger writes compact formatted logs and uses a [`tracing_subscriber::EnvFilter`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html).

```rust
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::prelude::*;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = NullSource::<u8>::new();
    let snk = NullSink::<u8>::new();

    connect!(fg, src > snk);

    // THIS IS NEVER LOGGED
    info!("starting flowgraph");

    // Default logger is created here
    Runtime::new().run(fg)?;

    // this is logged
    info!("flowgraph finished");

    Ok(())
}
```


## Initialize Logging Early

If you want to use FutureSDR logging before constructing a runtime, call `futuresdr::runtime::init()` yourself:

```rust
use futuresdr::prelude::*;

fn main() -> Result<()> {
    futuresdr::runtime::init();

    info!("parsing arguments before runtime construction");

    let rt = Runtime::new();
    // build and run flowgraphs

    Ok(())
}
```

Calling `init()` more than once is harmless. If another subscriber is already installed, FutureSDR leaves it in place.

## Log Level

FutureSDR's default logger gets its log level from the runtime config key `log_level`. On native targets, config can come from the usual FutureSDR config files or environment variables described in [Running Applications](running_apps.md#configuration).

```toml
log_level = "debug"
```

For per-module filtering, set `FUTURESDR_LOG`. This uses `EnvFilter` syntax and overrides the default directive from `log_level`:

```bash
# set the default log level
FUTURESDR_LOG=warn cargo run

# disable logs from one module
FUTURESDR_LOG=lora::frame_sync=off cargo run --bin rx

# combine a default level with a module-specific rule
FUTURESDR_LOG=info,lora::decoder=off cargo run --release --bin rx
```

The accepted config values are tracing level filters such as `off`, `error`, `warn`, `info`, `debug`, and `trace`.

## Compile-Time Filters

> [!WARNING]
> By default, FutureSDR enables feature flags that apply compile-time tracing filters: `tracing_max_level_debug` and `tracing_release_max_level_info`.
>
> These filters remove more verbose log statements at compile time. In debug builds, `trace` messages are disabled. In release builds, messages more detailed than `info` are disabled.
>
> The filters are transitive. If your application needs more detailed logs, disable FutureSDR's default features and enable the features you need explicitly:
>
> ```toml
> [dependencies]
> futuresdr = { version = "...", default-features = false, features = ["audio", "seify"] }
> ```

Runtime filters such as `FUTURESDR_LOG=trace` cannot show messages that were removed by compile-time filters.

## Custom Subscriber

Applications can install their own tracing subscriber before constructing a runtime. In that case, FutureSDR does not replace it, and FutureSDR's logging config is not applied by the default logger.

```rust
use futuresdr::prelude::*;
use futuresdr::tracing::level_filters::LevelFilter;
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

fn main() -> Result<()> {
    let format = fmt::layer()
        .with_level(true)
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .compact();

    let filter = EnvFilter::from_env("MY_APP_LOG").add_directive(LevelFilter::INFO.into());

    tracing_subscriber::registry()
        .with(filter)
        .with(format)
        .init();

    info!("custom subscriber installed");

    let rt = Runtime::new();
    // build and run flowgraphs

    Ok(())
}
```

This is the right approach when an application already has its own logging policy, formatting, file logging, telemetry exporter, or framework integration.

