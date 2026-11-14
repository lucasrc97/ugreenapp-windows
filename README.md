# ugreenapp

Rust CLI for a Linux/Windows port of UGREEN Connect.

Current status:
- real `UG1` control works on `UGREEN HiTune Max5c` over Linux RFCOMM; Windows RFCOMM transport added (see Build)
- verified commands: `query-info`, `query-firmware`, `set-anc`, `set-eq`, `set-prompt-language`, `set-prompt-volume`
- the main code in this repository was created with AI assistance and then tested/refined manually

## Layout

Cargo workspace under `crates/`:

- `ugreen-core` (lib): protocol frames and the `Transport` trait, with one
  transport per OS (`transport/linux.rs`, `transport/windows.rs`)
- `ugreen-cli` (bin): command line and REPL
- `ugreen-gui` (bin): desktop GUI (egui / eframe)

## Build

Linux:

```bash
cargo build --release
./target/release/ugreen-cli
```

Windows, cross-compiled from Linux with [cargo-xwin](https://github.com/rust-cross/cargo-xwin):

```bash
cargo xwin build --release --target x86_64-pc-windows-msvc
./target/x86_64-pc-windows-msvc/release/ugreen-cli.exe
```

On Windows the headset must already be paired in the system Bluetooth
settings. `scan` lists paired supported headsets; nothing is discovered or
paired by this tool. Commands connect on RFCOMM channel 1 and fall back to the
Serial Port channel advertised over SDP. `services` prints the headset's RFCOMM
channels, and `--verbose` logs the raw bytes exchanged (to stderr).

Checked on Windows with a HiTune Max5c (firmware 0.5.7): `scan`, `services`,
`query-info`, `query-firmware`, `anc off|transparency|deep`, `eq bass|treble`,
`vol`, `lang english|chinese`.

## GUI

```bash
./target/release/ugreen-gui            # Linux
./target/x86_64-pc-windows-msvc/release/ugreen-gui.exe
./target/release/ugreen-gui --demo     # pretend headset, no Bluetooth needed
```

Lists the paired devices, connects to a HiTune Max5c (the only model the GUI
accepts) and shows firmware, battery, noise control, equalizer preset and voice
prompt language / volume. Every change is followed by a fresh read, so the
window shows what the headset stored. Bluetooth runs on a worker thread; the
window stays responsive while a command is in flight. There is no factory
reset in the GUI. It is pinned to eframe 0.33, the newest release that builds
with the Rust version this project uses.

## CLI

Useful commands:
- `scan`
- `connect max5c`
- `info`
- `fw`
- `anc off|transparency|light|medium|deep|adaptive`
- `eq bass|pop|classic`
- `lang english|chinese`
- `vol 1..15`

Use `--json` with any command for full state output, for example:

```bash
./target/release/ugreen-cli --json query-info
```

Inside the REPL:

```text
scan --json
info --json
```

`factory-reset` is intentionally gated behind `--yes-really-reset-device`.

## Docs

- `docs/earbuds-desktop-bridge.md`
- `docs/desktop-app-shape.md`
- `docs/protocol-layer.md`
