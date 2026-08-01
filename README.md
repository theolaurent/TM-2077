# TM-2077

[![CI](https://github.com/theolaurent/TM-2077/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/theolaurent/TM-2077/actions/workflows/ci.yml)
[![Deployment](https://github.com/theolaurent/TM-2077/actions/workflows/deploy.yml/badge.svg?branch=main)](https://github.com/theolaurent/TM-2077/actions/workflows/deploy.yml)


A metronome / tuner combo, inspired by a well known brand's device.
Featuring chromatic, guitar and quarter tone modes.

Written in Rust, it runs as a native desktop app and in the browser
(deployed at https://theolaurent.github.io/TM-2077/).

## Build native

Requires a Rust toolchain (stable, edition 2024), plus:

- `pkg-config` and the ALSA development headers (`libasound`/`alsa-lib`) — cpal
  builds its native audio backend against them.
- X11, `libxkbcommon`, OpenGL, and Vulkan loader libraries, for the egui/eframe
  window and renderer.

```sh
cargo run
```

## Build web

Requires a Rust toolchain (stable, edition 2024), the `wasm32-unknown-unknown`
target (`rustup target add wasm32-unknown-unknown`), and Trunk.

```sh
trunk serve
```
