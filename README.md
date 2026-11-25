# Rezo

Rezo sends what your computer is playing to the speakers around your home, all at once: Sonos, UPnP/DLNA renderers, other PCs running a small receiver, and phones or tablets through a web page. Each speaker gets its own channel strip (gain, EQ, effects, pan, delay) so rooms can be balanced and lined up by ear.

![Console view](docs/images/console.jpg)

<p>
  <img src="docs/images/rooms.png" alt="Rooms view" width="332">
  <img src="docs/images/mixer.png" alt="Audio mixer window" width="460">
</p>

## Features

- **Sources**: everything the computer plays (WASAPI loopback on Windows, a loopback device such as BlackHole on macOS) or a single application (Windows 10 2004+).
- **Outputs**
  - Sonos and UPnP/DLNA renderers, fed with a continuous 48 kHz WAV stream over HTTP.
  - Native receivers over UDP/RTP with Opus in low-delay mode (`tools/rezo-recv`), for the lowest latency.
  - Any phone or tablet: scan a QR code, the browser becomes a speaker (WebSocket + AudioWorklet).
- **Per-speaker processing**: gain, 3-band tone EQ, a six-band equalizer with 20 presets and a draggable curve (60 Hz to 15 kHz, ±12 dB, bypass that keeps the curve), bass boost, presence, loudness, stereo width, compressor, noise gate, pan, channel mode, phase invert, delay from -500 to +500 ms, limiter.
- **Speaker volume**: a second fader on each Console strip sets the speaker's own volume (the one the Sonos app shows) or the receiver's output level.
- **Sync**: one button groups the active Sonos speakers the way the Sonos app does, so they play sample-aligned, and delays the other speakers to play with them.
- **Sonos line-in**: for games, a Sonos with a line-in (Five, Port, Amp) can play the PC through a cable, the lowest delay a Sonos offers; Sync groups the other Sonos with it.
- **Rooms and scenes**: group speakers into rooms, save complete mixes and recall them in one click.
- **Views**: Rooms, Map (drag speakers between rooms) and Console (mixing desk with MIDI Learn), as tabs of one window that adapts to its width, down to 360 px.
- **Media controls** (Windows): play/pause, previous, next and a seek bar for whatever the PC is playing (Spotify, a browser, a media player).
- **Audio mixer window**: volume and mute for each PC output (wired, Bluetooth, HDMI, USB) and each application, and switching the default output.
- **Smart Output** (optional): mutes the PC output while network speakers play and restores that exact output afterwards.
- **French and English** interface, switchable in Settings.
- **Updates**: Rezo tells you when a new release is out and can download it into its own folder, check it against the release's SHA-256 checksums and restart on it.

## Latency

| Path | Typical delay |
|---|---|
| Native receiver (Opus over UDP) | 10 to 25 ms |
| Web browser | 40 to 150 ms, depending on the device |
| Sonos over the network | about 350 ms (measured with a microphone on a Sonos Play:1, capture to sound) |
| Sonos line-in (cable from the PC) | set by the line-in Audio Delay option of the Sonos app |
| Other UPnP renderers | depends on the device, often 1 s or more |

A Sonos keeps a buffer of its own that Rezo cannot remove; "Lowest Sonos delay" (Settings, on by default) keeps Rezo's own share as small as possible. Rezo keeps the stream open while a speaker is enabled, so audio starts after that buffer only, without reloading.

## Download

Get the latest version from the [Releases page](https://github.com/theappstore-cell/Rezo/releases/latest):

| File | For |
|---|---|
| `Rezo-<version>-windows-x64-setup.exe` | Windows 10/11 on Intel or AMD |
| `Rezo-<version>-windows-arm64-setup.exe` | Windows 11 on ARM (Snapdragon laptops) |
| `Rezo-<version>-windows-<arch>-portable.exe` | Windows, single executable, nothing to install |
| `Rezo-<version>-macos-universal.dmg` | macOS 11 or later, Apple Silicon and Intel Macs |
| `rezo-recv-<version>-*` | native receiver for another computer |

Every file is built by GitHub Actions from this repository and comes with a signed build attestation and a SHA-256 checksum: see [SECURITY.md](SECURITY.md) to verify a download. The binaries are not code-signed yet, so Windows SmartScreen may warn on first launch ("More info" > "Run anyway"); on macOS, open the app once from System Settings > Privacy & Security > "Open Anyway".

### macOS setup

macOS does not let applications record the system output directly, so Rezo records a loopback device:

1. Install [BlackHole 2ch](https://github.com/ExistentialAudio/BlackHole) (free, open source).
2. In **Audio MIDI Setup**, create a **Multi-Output Device** with your speakers or headphones and BlackHole, and select it as the sound output.
3. Start Rezo and allow microphone access when asked: Rezo picks BlackHole automatically.

The audio mixer window, single-application capture and Smart Output are Windows-only.

## Requirements

- Windows 10 (2004 or later) or Windows 11, x64 or ARM64; or macOS 11 or later.
- Computer and speakers on the same local network. Allow Rezo on private networks when the firewall asks.

## Building

You need Rust (MSVC toolchain), the Visual Studio Build Tools with the Windows SDK, CMake and Node.js 20 or newer.

```powershell
npm install
npm run tauri build
```

The installer ends up in `src-tauri/target/release/bundle/nsis/`. For development, use `npm run tauri dev`. To get a standalone executable with a console for logs, run `cargo build --release --features console` in `src-tauri`.

The native receiver builds on Windows, macOS and Linux:

```powershell
cd tools/rezo-recv
cargo run --release -- --name "Office PC" --port 5004
```

More details on building, firewall rules and network tuning are in [docs/BUILD.md](docs/BUILD.md). The wire formats are described in [docs/PROTOCOL.md](docs/PROTOCOL.md).

## How it works

```
WASAPI loopback ──► lock-free ring ──► router (MMCSS "Pro Audio" while streaming)
                                         │  per speaker: gate → EQ → tone → compressor → width
                                         │               → pan/mode/phase → gain → delay → limiter
                                         ├──► Opus over UDP/RTP   (native receivers)
                                         ├──► WAV over HTTP       (Sonos / UPnP)
                                         └──► PCM over WebSocket  (browsers)
```

The engine is written in Rust (`src-tauri`), the interface in React and TypeScript (`src`), packaged with Tauri 2. Parameters reach the audio thread through atomics and audio moves through SPSC ring buffers, so the real-time path never locks or allocates.

When no speaker is enabled, Rezo opens no audio stream, registers no real-time task and its router thread sleeps. Audio devices, including Bluetooth headsets, can go idle normally.

## Project layout

```
src/                 React interface
src-tauri/src/
  audio/             capture, DSP, router, encoders, output switch, mixer
  network/           discovery, Sonos/UPnP, UDP streamer, web receiver, HTTP server
  state/             persistent configuration
  engine.rs          orchestration
src-tauri/web/       browser receiver page and AudioWorklet
tools/rezo-recv/     reference native receiver
docs/                build guide and protocol notes
```

## License

GPL-3.0. See [LICENSE](LICENSE).
