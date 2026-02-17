# Logging

> [!WARNING]
> By default, FutureSDR sets feature flags that disable `tracing` level log messages in debug mode and everything more detailed than `info` in release mode. This is a *compile time* filter!
>
> Also, these flags are transitive! If you want more detailed logs in your application, disable default features for the FutureSDR dependency.
> ```rustc
> [dependencies]
> futuresdr = { version = ..., default-features=false, features = ["foo", "bar"] }
> ```

Tracing macros from prelude.

Custom log handler or default from `futuresdr::runtime::init()`.

