# Build, firewall and network guide

## 1. Building

### Windows 10 2004+ / 11

| Tool | Why |
|---|---|
| Rust stable (`x86_64-pc-windows-msvc`), <https://rustup.rs> | engine |
| Visual Studio Build Tools, "Desktop development with C++" + Windows 10/11 SDK | MSVC linker, `windows` crate |
| CMake on `PATH` | the `opus` crate builds libopus |
| Node.js 20+ | interface (Vite, Tailwind) |
| WebView2 Runtime | ships with Windows 11; the installer fetches it otherwise |

```powershell
npm install
cd src-tauri; cargo test; cd ..
npm run tauri dev       # development
npm run tauri build     # src-tauri/target/release/bundle/nsis/Rezo_<version>_x64-setup.exe
```

`cargo build --release` in `src-tauri` also produces a standalone `Rezo.exe` with the interface embedded; add `--features console` to keep a console window with logs.

ARM64 (Windows on ARM) from an x64 machine needs the "MSVC ARM64 build tools" component of Visual Studio:

```powershell
rustup target add aarch64-pc-windows-msvc
npx tauri build --target aarch64-pc-windows-msvc --bundles nsis
```

### macOS 11+

Xcode command line tools, Rust, CMake (`brew install cmake`) and Node.js 20+. Build libopus from source so the app does not depend on Homebrew:

```bash
export LIBOPUS_STATIC=1 LIBOPUS_NO_PKG=1
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm ci
npx tauri build --target universal-apple-darwin --bundles app,dmg
```

### Releases

Published binaries come from `.github/workflows/release.yml` only (Windows x64 and ARM64, macOS universal), with checksums, build attestations and SBOMs. See [SECURITY.md](../SECURITY.md).

Push a `v*` tag (or run the workflow by hand with a tag name): it builds everything and creates a draft release, which you then publish from GitHub. A tag whose release already exists is skipped.

### Reference native receiver

Another PC, a Raspberry Pi, a Mac:

```powershell
cd tools/rezo-recv; cargo run --release -- --name "Living room PC" --port 5004 --buffer-ms 20
```

Configuration lives in `%APPDATA%\Rezo\config.json` (atomic writes, `.bak` copy if the file cannot be read). Logs go to `%APPDATA%\Rezo\rezo.log`; set `RUST_LOG=rezo_lib=debug` for more detail.

## 2. Windows Firewall

Rezo has to accept incoming TCP connections (Sonos fetching the WAV stream, browsers) and UDP replies (SSDP, mDNS). Without a rule Windows asks on first launch: tick **Private networks**.

- The NSIS installer adds a program rule limited to the private profile when installing for all users (a per-user install has no rights to change the firewall, so Windows asks on first launch).
- Otherwise, as administrator: `powershell -ExecutionPolicy Bypass -File scripts\firewall.ps1`.
- The network must be classified as **Private** (Settings > Network & Internet > Properties), or Windows blocks incoming traffic even with the rule.
- HTTP port: 49494 by default (Settings). If it is taken, Rezo picks a free port and the QR code shows it.

## 3. Discovery (SSDP / mDNS)

| Symptom | Likely cause | Fix |
|---|---|---|
| No speaker found | PC and speakers on different VLANs or SSIDs | same subnet, or enable an mDNS/SSDP relay on the router |
| Only some speakers show up | client isolation ("AP isolation", guest network) | turn it off for that SSID |
| Discovery comes and goes on Wi-Fi | IGMP snooping without a querier | enable a querier or disable snooping on that VLAN |
| Nothing found with a VPN active | full-tunnel VPN capturing the traffic | Rezo only uses the physical LAN adapter, but a full tunnel can still block local traffic: split it or turn it off |
| Adding by IP works, scanning does not | multicast filtered | use "By IP address" (Sonos: port 1400) |

A scan takes about 1.2 s and repeats every 30 to 120 s depending on how stable the network is; mDNS listens continuously.

## 4. Latency and bandwidth

**Native UDP path**

- Wired Ethernet for receivers when possible. On Wi-Fi: 5 GHz, WMM on, adapter power saving off, few devices on the channel.
- Opus frame of 2.5 ms on a clean network, 5 ms otherwise; keep frame redundancy on over Wi-Fi.
- "Low-latency audio period" (off by default) keeps a silent `IAudioClient3` render stream open while streaming so the Windows audio engine runs at about 3 ms instead of 10 ms. It changes the period for every application on that output and is never applied to Bluetooth outputs.
- Disable audio enhancements (APO, Dolby, Sonic) on the output device: they add latency and jitter to the capture.

**Bandwidth per speaker**: Opus 160 kb/s (twice that with redundancy), Sonos/UPnP WAV 1.54 Mb/s, web PCM 1.54 Mb/s.

## 5. What Rezo changes on a PC

- **Idle** (no speaker enabled): no audio stream open, no MMCSS task, no 1 ms timer unless the window is visible, router thread asleep.
- **Smart Output** (off by default): mutes the default output while streaming and restores that exact output by id, including after a crash; if the output has disappeared, the restore happens when it comes back.
- **MIDI**: ports are opened only when bindings exist or MIDI Learn is on, and released afterwards (a Windows MIDI port belongs to a single application).
- **Network**: SSDP and mDNS only on the physical LAN adapter (VirtualBox, Hyper-V, WSL and VPN adapters are skipped).
- **Mixer window**: polls outputs once per second while open and visible, never queries the hands-free profile of Bluetooth headsets, and changes the default output only on click.

## 6. Lining up several speakers

1. Press **Sync** in the Console. The active Sonos speakers join one Sonos group, as when grouped in the Sonos app: one of them receives Rezo's stream (or plays its line-in) and the others follow it sample for sample, without drifting apart. Press **Sync** again to split the group; each speaker then gets its own stream back.
2. Sync also delays the other active speakers (native receivers, browsers) so they play with the slowest one, from per-type estimates: about 300 ms for a Sonos with "Lowest Sonos delay", the computed delay of a native receiver, the reported buffer of a browser. Fine-tune by ear with the **Sync** knob and the -1 ms / +1 ms buttons; a negative delay delays every other speaker instead. Values are stored per speaker and in scenes.
3. For games, use a Sonos with a line-in (Five, Port, Amp): connect the PC's headphone or line output to it, turn on **Line-in** on its strip, and set its line-in Audio Delay to the lowest value in the Sonos app. Rezo does not process that audio (the mix controls do not apply), and Smart Output stays off since the PC output feeds the cable. Sync groups the other Sonos with it.
4. Put the PC on Ethernet; on Sonos, wire one speaker rather than running everything over Wi-Fi.
5. Stereo pairs and home-theater sets are created in the Sonos app; Rezo only shows the main speaker.
6. A Sonos that belongs to a group made in the Sonos app is taken out of it when streaming starts. Use **Sync** to group speakers from Rezo instead.

## 7. Troubleshooting

| Problem | Things to check |
|---|---|
| No sound on a Sonos, UPnP error 701/714/800 | firewall or network profile: the speaker cannot reach `http://<pc-ip>:49494/s/<key>/stream/…`; open the browser receiver page from another device to check |
| Sonos stops after a few seconds | look at `rezo.log`: every stream end is logged with its reason; on a weak Wi-Fi link, turn off "Lowest Sonos delay" |
| No sound on the PC after a crash | start Rezo again (it restores the output it muted) |
| PC muted but nothing on the speakers | driver that applies the mute before loopback: turn Smart Output off |
| Crackles on a native receiver | larger buffer (`--buffer-ms 30`), 5 ms frames, check the Wi-Fi |
| A phone's browser cannot connect | same Wi-Fi, no client isolation, port allowed, QR code showing the right interface |
