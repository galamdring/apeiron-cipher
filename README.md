# Apeiron Cipher

[![CI](https://github.com/galamdring/apeiron-cipher/actions/workflows/rust.yml/badge.svg)](https://github.com/galamdring/apeiron-cipher/actions/workflows/rust.yml)
[![Release](https://github.com/galamdring/apeiron-cipher/actions/workflows/release-game.yml/badge.svg)](https://github.com/galamdring/apeiron-cipher/actions/workflows/release-game.yml)

| Platform | Status |
|----------|--------|
| Windows | [![Build Windows](https://github.com/galamdring/apeiron-cipher/actions/workflows/release-game.yml/badge.svg?job=Build+Windows)](https://github.com/galamdring/apeiron-cipher/actions/workflows/release-game.yml) |
| macOS x86_64 | [![Build macOS x86_64](https://github.com/galamdring/apeiron-cipher/actions/workflows/release-game.yml/badge.svg?job=Build+macOS+x86_64)](https://github.com/galamdring/apeiron-cipher/actions/workflows/release-game.yml) |
| macOS ARM64 | [![Build macOS ARM64](https://github.com/galamdring/apeiron-cipher/actions/workflows/release-game.yml/badge.svg?job=Build+macOS+ARM64)](https://github.com/galamdring/apeiron-cipher/actions/workflows/release-game.yml) |
| Linux x86_64 | [![Build Linux x86_64](https://github.com/galamdring/apeiron-cipher/actions/workflows/release-game.yml/badge.svg?job=Build+Linux+x86_64)](https://github.com/galamdring/apeiron-cipher/actions/workflows/release-game.yml) |
| Linux ARM64 | [![Build Linux ARM64](https://github.com/galamdring/apeiron-cipher/actions/workflows/release-game.yml/badge.svg?job=Build+Linux+ARM64)](https://github.com/galamdring/apeiron-cipher/actions/workflows/release-game.yml) |
| WASM | [![Build WASM](https://github.com/galamdring/apeiron-cipher/actions/workflows/release-game.yml/badge.svg?job=Build+WASM)](https://github.com/galamdring/apeiron-cipher/actions/workflows/release-game.yml) |

A procedurally generated open universe sandbox where knowledge is the only
progression that matters. Built with [Bevy](https://bevyengine.org/).

## Downloads

Pre-built binaries are available on the
[Releases](../../releases) page for Windows, macOS, Linux, and Web (WASM).

## Building from Source

Requires [Rust](https://www.rust-lang.org/tools/install) (stable).

```sh
cargo run --release
```

### Linux Dependencies

Linux users need the following libraries installed before building or running:

**Debian / Ubuntu:**

```sh
sudo apt-get install libudev-dev libasound2-dev libwayland-dev libxkbcommon-dev
```

**Fedora:**

```sh
sudo dnf install libudev-devel alsa-lib-devel wayland-devel libxkbcommon-devel
```

Pre-built Linux releases dynamically link against:
- `libudev` -- device events (gamepad hot-plug)
- `libasound2` -- audio output
- `libwayland-client` -- Wayland display
- `libxkbcommon` -- keyboard input

These are present on standard desktop installations (GNOME, KDE, etc.).
Minimal or server environments may need to install them.

## License

See [LICENSE](LICENSE) for details.
