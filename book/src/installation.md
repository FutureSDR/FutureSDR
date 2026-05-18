# Installation

Compiling and running FutureSDR applications requires at least a Rust toolchain.
The sections below walk you through setting up Rust and the additional tooling
needed for building native binaries and the web user interface.

## Install Rust

To install Rust, follow the [official instructions](https://www.rust-lang.org/tools/install).

FutureSDR requires the nightly Rust toolchain. The root crate uses nightly-only Rust features, and the Leptos-based web UI crates also enable Leptos' `nightly` syntax feature. The `rust-version` in `Cargo.toml` is only the minimum compiler version; the channel must still be nightly.

Install nightly with the standard development components:

```bash
rustup toolchain install nightly --component rustfmt clippy
```

For FutureSDR applications, either make nightly your default toolchain:

```bash
rustup default nightly
```

or set it per project:

```bash
cd <into your project or FutureSDR>
rustup override set nightly
```

The FutureSDR repository contains a `rust-toolchain.toml`, so `cargo` automatically selects nightly when run inside the checkout.

## Web GUI and Web SDR Applications

FutureSDR ships with pre-compiled web UIs, so you can use them without extra
tooling. If you want to extend or adapt the web UIs, install the
`wasm32-unknown-unknown` target:

```bash
rustup target add wasm32-unknown-unknown --toolchain nightly
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

## Windows Subsystem for Linux (WSL)

Alternatively, FutureSDR can be run on Windows using WSL (These steps are verified for Ubuntu 24.04).
- Open PowerShell as Administrator and run: <br/>`wsl --install -d Ubuntu-24.04`
- After installation, restart your PC.
- Open a Linux terminal, set up your username/password, and run: <br/>`sudo apt update && sudo apt upgrade`
- Install the core tools required for compiling Rust and C++ projects: <br/>`sudo apt install git build-essential cmake libfontconfig1-dev clang libclang-dev usbutils`
- Install Rust and make nightly the default toolchain:

  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  source $HOME/.cargo/env
  rustup toolchain install nightly --component rustfmt clippy
  rustup default nightly
  ```

- Depending on your SDR device, install the necessary drivers and firmware (e.g. for USRP devices):

  ```bash
  sudo apt install libuhd-dev uhd-host
  sudo uhd_images_downloader
  ```

- Install the bridge between SDR hardware and software, along with the audio bridge:

  ```bash
  sudo apt install libsoapysdr-dev soapysdr-tools soapysdr-module-uhd
  sudo apt install pulseaudio libasound2-dev libasound2-plugins
  ```
    **Note:** After installing `pulseaudio`, run `wsl --shutdown` in PowerShell and restart your terminal to activate the audio bridge. Ensure `ls -l /mnt/wslg/PulseServer` exists to enable audio support in examples.

- To prevent a `Segmentation Fault` during device discovery, disable the default Soapy audio module: <br/>`sudo mv /usr/lib/x86_64-linux-gnu/SoapySDR/modules0.8/libaudioSupport.so /usr/lib/x86_64-linux-gnu/SoapySDR/modules0.8/libaudioSupport.so.bak`

- To share your USB device with Linux, install `usbipd` on Windows:
  * Open PowerShell as Administrator and run: <br/>`winget install usbipd`
  * Plug in your SDR and identify the `VID:PID`: <br/>`usbipd list`
  * Bind and Attach (use Hardware-ID for port independence):

    ```PowerShell
    usbipd bind --hardware-id <VID:PID>
    usbipd attach --hardware-id <VID:PID> --auto-attach
    ```

- In your Linux terminal, verify if the hardware is visible:

  ```bash
  lsusb
  sudo uhd_find_devices
  ```

    **Note:** When you run `uhd_find_devices` or an SDR-based example for the first time, the USRP will download firmware and reset. This breaks the connection to WSL. You should return to Windows PowerShell and manually run this command again: <br/>`usbipd attach --hardware-id <VID:PID> --auto-attach`
 Once re-attached, your device will be stable until the device is physically disconnected.

- Finally, install FutureSDR and check if your setup is working by running `cargo build` in the FutureSDR directory: 

  ```bash
  git clone https://github.com/FutureSDR/FutureSDR.git
  cd FutureSDR
  cargo build --release
  ```