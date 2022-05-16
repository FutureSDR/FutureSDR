FutureSDR & Audio
=================

## Introduction

FutureSDR come with some blocks interfacing the [cpal crate](https://crates.io/crates/cpal) so as to interact with sound files and audio card.

To listen the rick.mp3 file, execute:
```sh
cd examples/audio/
cargo run --bin play-file --release
```

To listen a 440Hz sound, execute:
```sh
cd examples/audio/
cargo run --bin play-tone --release
```

To listen the rick.mp3 file in stereo mode with different sound levels, execute:
```sh
cd examples/audio/
cargo run --bin play-stereo --release
```
