# Installation

Compiling and running FutureSDR applications requires at least a Rust toolchain.
The sections below walk you through setting up Rust and the additional tooling
needed for building native binaries and the web user interface.

## Install Rust

To install Rust, follow the [official instructions](https://www.rust-lang.org/tools/install).

FutureSDR works with both the `stable` and `nightly` toolchains. The `nightly`
compiler enables a few performance optimizations and is required when you build
or modify the web UI, since it uses [Leptos](https://leptos.dev/), which
provides an [ergonomic syntax](https://book.leptos.dev/reactivity/working_with_signals.html?highlight=nightly#nightly-syntax)
behind a `nightly` feature flag.

> [!TIP]
> We recommend using the `nightly` Rust toolchain.

You can switch to `nightly` globally:

```bash
rustup toolchain install nightly
rustup default nightly
```

or only for your FutureSDR project:

```bash
rustup toolchain install nightly
cd <into your project or FutureSDR>
rustup override set nightly
```

## Web GUI and Web SDR Applications

FutureSDR ships with pre-compiled web UIs, so you can use them without extra
tooling. If you want to extend or adapt the web UIs, install the
`wasm32-unknown-unknown` target:

```bash
rustup target add wasm32-unknown-unknown
```

Install [Trunk](https://trunkrs.dev/), a build and packaging tool for Rust
WebAssembly projects, with Cargo or one of the
[other options](https://trunkrs.dev/#install) listed in their documentation:

```bash
cargo install --locked trunk
```


## Linux (Ubuntu)

- Clone the FutureSDR repository<br/>`git clone https://github.com/FutureSDR/FutureSDR.git`
- Optionally, install SoapySDR<br/>`sudo apt install -y libsoapysdr-dev soapysdr-module-all soapysdr-tools`
- Check if your setup is working by running `cargo build` in the FutureSDR directory.

## macOS

These instructions assume that you use [Homebrew](https://brew.sh) as your
package manager.
- Clone the FutureSDR repository<br/>`git clone https://github.com/FutureSDR/FutureSDR.git`
- Optionally, install SoapySDR<br/>`brew install soapysdr`
- Additional drivers are available in the [Pothos Homebrew tap](https://github.com/pothosware/homebrew-pothos/wiki).
- Check if your setup is working by running `cargo build` in the FutureSDR directory.

## Windows

- Clone the FutureSDR repository<br/>`git clone https://github.com/FutureSDR/FutureSDR.git`.
- Install [Visual Studio C++ Community Edition](https://visualstudio.microsoft.com/downloads/) (required components: Win10 SDK and VC++).

  Visual Studio does not add its binaries and libraries to the `PATH`.
  Instead, it offers various terminal environments, configured for a given toolchain.
  Please use the native toolchain for your system to build FutureSDR, e.g., *x64 Native Tools Command Prompt for VS 2022*.

For SoapySDR hardware drivers:
- [Miniconda](https://www.anaconda.com/docs/getting-started/miniconda/install/overview) for pre-built SDR drivers. The installer offers to add the binaries to your `PATH`. Do not check this option.
- After installation, open Anaconda Prompt application.
- Create an environment and activate it: <br/>`conda create -n sdr_env && conda activate sdr_env`
- Install SoapySDR: <br/>`conda install -c conda-forge soapysdr`
- Install necessary drivers (e.g. for USRP): <br/>`conda install -c conda-forge soapysdr-module-uhd` <br/>**Note:** Download FPGA images if using USRP: <br/>`uhd_images_downloader`
- Add the following to your *User Environment Variables*:

  | Variable | Value |
  | :--- | :--- |
  | <small>SOAPY_SDR_ROOT</small> | `C:\Users\<User>\miniconda3\envs\sdr_env\Library` |
  | <small>SOAPY_SDR_PLUGIN_PATH</small> |`C:\Users\<User>\miniconda3\envs\sdr_env\Library\lib\SoapySDR\modules0.8` |
  | <small>LIB</small> | `C:\Users\<User>\miniconda3\envs\sdr_env\Library\lib` |
  | <small>PATH (Append this one)</small>| `C:\Users\<User>\miniconda3\envs\sdr_env\Library\bin` |

- For verification, restart a new terminal and run `SoapySDRUtil --info`. Check if your hardware (e.g., uhd) is listed under `Available factories`.

- Check if your setup is working by running `cargo build` in the FutureSDR directory.