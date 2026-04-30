# Performance Measurement

FutureSDR performance work usually happens at two levels:

- use [Mocker](mocker.md) to benchmark one block implementation,
- use the `perf/` applications to benchmark complete flowgraphs and scheduler or buffer configurations.

Always measure release builds. Debug builds are useful while developing, but they do not represent runtime performance.

## Block Microbenchmarks

For a single block, use `Mocker`. It runs the block directly, without a scheduler and without a full `Flowgraph`, so the benchmark mostly measures the block's `work()` implementation and the buffer operations it performs.

This is the right tool for comparing implementation choices inside one block, checking how performance changes with input size, or writing a Criterion benchmark around a small processing kernel. The repository's [apply benchmark](https://github.com/FutureSDR/FutureSDR/blob/main/benches/apply.rs) is a compact example:

```bash
cargo bench --bench apply
```

`Mocker` is not a replacement for full-flowgraph benchmarks. It intentionally removes scheduling, message routing between blocks, and end-to-end stream topology effects.

## Parameter Sweeps

The `perf/` directory contains standalone benchmark applications for measuring complete configurations. These examples are useful when the question is about scheduler choice, buffer behavior, number of stages, number of pipes, sample counts, or other flowgraph-level parameters.

Many of the directories contain a `Makefile` that iterates over a parameter grid, writes CSV files to `perf-data/`, and provides helper targets for selected configurations:

```bash
cd perf/null
make
```

Inspect the local `Makefile` before running a sweep. Some benchmarks run for a long time, and the parameter ranges are intentionally broad.

## Profiling One Configuration

To understand where time is spent in one specific configuration, profile that configuration directly. [Samply](https://github.com/mstange/samply) works well for this because it records a profile and opens an interactive view in the browser:

```bash
samply record -- cargo run --release
```

For an independent example workspace, run it from that directory or pass the manifest path:

```bash
samply record -- cargo run --release --manifest-path=perf/null/Cargo.toml -- --config=flow
```

Enable debug symbols for release builds so the profile contains useful function names and source locations:

```toml
[profile.release]
debug = true
```

Add this to the `Cargo.toml` of the workspace you are profiling. For the root crate, that is the repository root. For a benchmark under `perf/`, it is usually the `Cargo.toml` inside that benchmark directory.

The flame graph view is often the most useful starting point. Look for unexpectedly large functions, allocation-heavy paths, synchronization overhead, and time spent outside the block code when the goal is to tune scheduling or buffering.

## Stable Measurements

For reproducible results, reduce system noise. One practical approach on a systemd-based Linux machine is to move normal system work onto a small CPU set and run the benchmark on the remaining CPUs.

First, restrict the normal system slices to the CPUs reserved for the operating system:

```bash
SYSTEM_CPUS=0,1

sudo systemctl set-property --runtime -- user.slice AllowedCPUs=${SYSTEM_CPUS}
sudo systemctl set-property --runtime -- system.slice AllowedCPUs=${SYSTEM_CPUS}
sudo systemctl set-property --runtime -- init.scope AllowedCPUs=${SYSTEM_CPUS}
```

Then start the benchmark in its own transient unit on the CPUs reserved for the measurement:

```bash
sudo systemd-run --uid=$(id -u) --slice=sdr --wait -P -p AllowedCPUs=2,3 -d -- cargo run --release
```

The `perf/migrate-processes.sh` and `perf/revert-processes.sh` scripts show the same pattern for the repository benchmarks. The settings are runtime-only, but reset them after a measurement run or reboot before using the machine normally.
