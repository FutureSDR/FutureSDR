Logging Example (logging)
========================

## Introduction

This example demonstrates how to set up a logging system in FutureSDR. It shows how to format logs for multi-threaded applications and how to control log visibility using environment variables.

## How it works:
1. Formats the Output: The code configures the logs to show the message together with the 'Thread ID' and 'Thread Name'.
2. Sets a Minimum Detail Level: By default, the code is hardcoded to show at least DEBUG level messages.
3. Environment Control: It looks for an environment variable named `FOO_LOG`. You can use this to increase the detail (e.g., to `trace`) or decrease it (e.g., to `warn`).
4. Monitors the Flowgraph: It logs a message when the flowgraph starts and another one when it finishes. Then it monitors the total elapsed time.

## How to run:
1. Go to the example directory and run it:

```sh
cargo run --release
  ```

* You will see `INFO` messages and the flowgraph timing in the terminal. Even if you don't set any variables, the code's internal `DEBUG` directive ensures you see the basic operations.

2. If you want to adjust the details of the messages:
```sh
set FOO_LOG={logging_level} && cargo run --release
  ```

* In this example, the `.add_directive(LevelFilter::DEBUG.into())` line acts as a floor. It forces the log level to be at least `DEBUG`.
* Decreasing Detail: Setting `FOO_LOG=warn` in your terminal will not hide `INFO` messages because the code's internal directive overrides it. To gain full control from the terminal, the `.add_directive` part would need to be removed.
* Increasing Detail: Setting it to `trace` will also not work in this build, as FutureSDR statically disables `TRACE` level logs (as seen in the `static max level is info` warning).