# Apeiron Cipher

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
