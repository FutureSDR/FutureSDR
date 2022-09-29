FutureSDR Audio Examples
========================

## Introduction

FutureSDR come with some blocks interfacing the [cpal
crate](https://crates.io/crates/cpal) so as to interact with sound files and
audio card.

## Play Tone

To play a 440Hz tone through a 48kHz mono `AudioSink`, execute:
```sh
cargo run --bin play-tone
```

## Play File

To listen the `rick.mp3` file, execute:
```sh
cargo run --bin play-file
```

This detects the sample rate of the audio file (in this case 44.1kHz) and number
of channels (in this case 1, i.e., mono) and tries to open an `AudioSink` with
the corresponding parameters to play the file.

## Play File Resampled in Stereo

To listen the `rick.mp3` file in stereo with different left/right sound levels
and resampled to 48kHz, execute:

```sh
cargo run --bin play-stereo
```

This detects the sample rate of the audio file (in this case 44.1kHz) and
resamples it to 48kHz. It, furthermore, makes sure that the input file is a mono
and maps it to stereo with different audio levels.

## Play selectable tone

To play somes tones through a 48kHz mono `AudioSink`, execute:
```sh
cargo run --bin play-selectable-tone
```

This is really to illustrate the usage of the `Selector` block with interactive usage of `Pmt` message.
Also the drop policy is configurable so as to experiment its effects on CPU and buffer usage.