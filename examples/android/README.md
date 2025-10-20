# Android Example

This example shows how to create an Android app that uses FutureSDR.
It implements a simple FM receiver using the RTL-SDR.

## Getting Started

- Download and install [Android Studio](https://developer.android.com/studio)
- Use tools -> SDK Manager to download the Android platform, NDK, and build tools. (This project uses Android 29 with NDK 28.2.13676358.)
- Build the [FutureSDR Android toolchain](https://github.com/FutureSDR/android-sdr-toolchain)
- Adapt the `jni` symbolic link (`./FutureSDR/app/src/main/jni`) in the Android app to your toolchain directory.
- Adapt the Cargo config (`./.cargo/config.toml`) to point to your Android SDK.
- Adapt the paths in the `build.sh` script to point to you Android SDK.
- Run the `build.sh` script.
- Open the FutureSDR Android project (`./FutureSDR/`) in Android Studio.
- Build and install the app in Android Studio.
- Close the app. Plugin the SDR. Start the app. You should be asked for permission to access the RTL-SDR and hear radio once it is granted.

