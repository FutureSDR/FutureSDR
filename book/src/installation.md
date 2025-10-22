# Installation

## Install Rust

To install Rust, we recommend following the [instructions](https://www.rust-lang.org/tools/install) on their website.

FutureSDR works with `stable` and `nightly` Rust versions.
However, `nightly` allows for a few more performance optimizations and might, therefore, be preferred.

In addition, working on the web GUI (i.e., extending and recompiling the frontend) requires nightly.
This is due to [Leptos](https://leptos.dev/), our GUI framework of choice.
It offers a nicer syntax with `nightly`, which we use in our frontend code.

You can switch to `nightly` globally

```admonish info
```

```bash
rustup toolchain install nightly
rustup default nightly
  ```

or only for your FutureSDR project

```bash
rustup toolchain install nightly
cd <into your project or FutureSDR>
rustup override set nightly
```

## Web GUI and Web SDR Applications

To compile the web frontend and web SDR applications, the `wasm32-unknown-unknown` target is required.
You can install it with

```bash
rustup target add wasm32-unknown-unknown
```

All web frontends and examples are compiled with [Trunk](https://trunkrs.dev/), a build and packaging tool for Rust WebAssembly projects.
You can install it with

```bash
cargo install --locked trunk
```

or one of the [other options](https://trunkrs.dev/#install) documented on their website.


## Linux (Ubuntu 23.10)

- Clone the FutureSDR repository<br/>`git clone https://github.com/FutureSDR/FutureSDR.git`
- Optionally, install SoapySDR<br/>`sudo apt install -y libsoapysdr-dev soapysdr-module-all soapysdr-tools`
- Check, if your setup is working by running `cargo build` in the FutureSDR directory.
- Continue, for example, with the included [applications](/learn/examples).

## macOS

These instructions assume that you use the [Homebrew](https://brew.sh) as package manager.
- Clone the FutureSDR repository<br/>`git clone https://github.com/FutureSDR/FutureSDR.git`
- Optionally, install SoapySDR<br/>`brew install soapysdr`
- Additional drivers are available in the [Pothos Homebrew tap](https://github.com/pothosware/homebrew-pothos/wiki).
- Check, if your setup is working by running `cargo build` in the FutureSDR directory.
- Continue, for example, with the included [applications](/learn/examples).

## Windows

- Install [Visual Studio C++ Community Edition](https://visualstudio.microsoft.com/downloads/) (required components: Win10 SDK and VC++).

  Visual Studio does not add its binaries and libraries to the `PATH`.
  Instead, it offers various terminal environments, configured for a given toolchain.
  Please use the native toolchain for your system to build FutureSDR, e.g., *x64 Native Tools Command Prompt for VS 2022*.

For SoapySDR hardware drivers:
- [PothosSDR](https://downloads.myriadrf.org/builds/PothosSDR/) for pre-built SDR drivers.
  The installer offers to add the libraries to your `PATH`. Make sure to check this option.
- Install [bindgen dependencies](https://rust-lang.github.io/rust-bindgen/requirements.html#windows).
- Run `volk_profile` on the command line.

PothosSDR comes with many SoapySDR modules. Some of them require further software and services, which can cause issues when scanning for available devices.
If you run into this issue, either (1) use a filter to specify the driver manually or (2) move the problematic library to a backup folder outside the search path.
The libraries are, by default, at `C:\Program Files\PothosSDR\lib\SoapySDR\modules0.8`.
If, for example, SDRplay or UHD causes issues, move `sdrPlaySupport.dll` or `uhdSupport.dll` to a backup folder.

- Check, if your setup is working by running `cargo build` in the FutureSDR directory.
- Continue, for example, with the included [applications](/learn/examples).
