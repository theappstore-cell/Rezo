# Rezo protocols

## 1. Native: Opus over UDP/RTP (`network/udp_streamer.rs`)

Discovery: mDNS `_rezo._udp.local.` with TXT records `id=<identifier>` and `name=<display name>`; the service port is the receiver's UDP port. `tools/rezo-recv` announces itself this way. Receivers can also be added by IP and port (default 5004).

Packet layout (big-endian):

| bytes | field |
|---|---|
| 0 | `0x80` (RTP v2, no padding, extension or CSRC) |
| 1 | bit 7 = **M** (first packet after silence), bits 0-6 = payload type **111** |
| 2-3 | sequence number, incremented for every packet sent |
| 4-7 | timestamp, **48 kHz** clock, +120 (2.5 ms) or +240 (5 ms) per frame |
| 8-11 | SSRC (FNV hash of the speaker id) |
| 12 | `flags`: bit 0 = previous frame included |
| 13-14 | `prev_len` (u16), 0 when there is no redundancy |
| 15… | previous Opus frame (`prev_len` bytes), then the current Opus frame |

- Audio: 48 kHz stereo, `OPUS_APPLICATION_RESTRICTED_LOWDELAY`, CBR (160 kb/s by default), 2.5 or 5 ms frames.
- **Keep-alive**: after 300 ms of digital silence the sender stops encoding and sends a **12-byte** packet (header only) every 20 ms. The receiver fills the gap from the timestamps. The next audio packet carries **M=1**: reset the decoder and skip loss concealment.
- **Feedback (optional)**: once per second the receiver may send back `RZFB` + received count (u16 BE) + lost count (u16 BE). After 60 s without reported loss the sender drops the redundant frame (half the bitrate); it turns it back on at the first loss or when feedback stops.
- **Volume**: when the receiver volume changes, and every 2 s, the sender sends `RZVL` + volume (u16 BE, 0 to 100); the receiver applies it as its output gain. Receivers that predate it drop the packet, since it is not RTP v2.
- **Loss**: if `seq` jumps by 2 and the packet carries the previous frame, decode it then the current one; otherwise use Opus PLC (`decode(None)`). Reference implementation: `tools/rezo-recv/src/main.rs`.
- Recommended receiver buffer: 4 to 15 ms (20 ms by default in the reference receiver).

## 2. Sonos / UPnP (`network/sonos_upnp.rs`)

- Discovery: SSDP `M-SEARCH` (MX 1, sent twice 300 ms apart) for `urn:schemas-upnp-org:device:ZonePlayer:1` and `MediaRenderer:1`, from one socket per private IPv4 interface of the LAN adapter, followed by a GET of the device description (`roomName`, `modelName`, `UDN`, AVTransport and RenderingControl `controlURL`, and whether an `AudioIn` service, a line-in, exists).
- Groups: `ZoneGroupTopology#GetZoneGroupState` gives visible and hidden members (stereo pairs, sub, surrounds), the group and its coordinator.
- Stream: `GET http://<pc-ip>:<port>/s/<key>/stream/<id>.wav` returns 16-bit 48 kHz stereo PCM with a `0xFFFFFFFF`-sized WAV header and an endless chunked body: 50 ms of pre-roll (200 ms with "Lowest Sonos delay" off), then live audio. Without the per-install key in the path the server answers `404`.
  - The pre-roll is ±1 LSB, not digital silence: a Sonos holds its answer to `Play` until its probe of the stream has seen a non-zero sample, for up to 10 s.
  - Pre-roll and delay, measured with a microphone on a Sonos Play:1 (capture to sound, through Rezo): 50 ms gives 354 ms, 200 ms gives 470 ms.
  - `HEAD` answers with headers only and never touches the active stream.
  - A `Range` request past the bytes already served (plus 2 MiB) is a tag lookup (a Sonos asks for 16 MiB or 3.7 GB right after connecting): `416`, and the active stream is left alone.
  - A `Range` request within the bytes already served is a resume: `206`, live PCM aligned on 4-byte frames, replacing the previous connection.
  - A new full `GET` replaces the previous connection (generation counter).
- Synchronized start: phase 1 in parallel `[BecomeCoordinatorOfStandaloneGroup]`, `Stop`, `SetAVTransportURI` (DIDL-Lite metadata); phase 2 in parallel `Play`.
- Speaker volume: `RenderingControl#GetVolume` / `SetVolume` (`Channel` Master, 0 to 100), read every 4 s while the window is visible.
- Sync: each other active Sonos receives `SetAVTransportURI` with `x-rincon:<coordinator UUID>` and plays the coordinator's audio in sync; leaving the group is `BecomeCoordinatorOfStandaloneGroup`.
- Line-in: `BecomeCoordinatorOfStandaloneGroup`, `SetAVTransportURI` with `x-rincon-stream:<own UUID>`, `Play`. No stream from Rezo; Smart Output stays off, since the PC output feeds the cable.
- Transport events (GENA `SUBSCRIBE`) report stops; if the renderer still holds Rezo's URL a single `Play` resumes it. Polling (`GetTransportInfo` + `GetMediaInfo`) is used when events are unavailable.

## 3. Web receiver (`network/web_receiver.rs`, `web/receiver.html`, `web/worklet.js`)

- `GET /r/<key>`: page; `GET /r/worklet.js`: AudioWorklet; `GET /ws?key=<key>&id=<client id>&name=<name>`: WebSocket.
- Server to client: text `{"t":"hello","rate":48000,"ch":2}`, then **binary** messages of 960 bytes (240 interleaved stereo Int16 little-endian frames, 5 ms), and text `{"t":"vol","v":<0-100>}` whenever the receiver volume is set from Rezo.
- Client to server: text `{"t":"stats","buf":<buffered ms>}`, used to display the estimated latency.
- Each browser becomes a speaker (`web-<id>`), remembered and enabled automatically on its first connection.
- The worklet runs an adaptive jitter buffer (30 to 160 ms depending on underruns), linear resampling when the audio context is not at 48 kHz, and a ±0.3 % playback-rate servo.

## 4. Tauri events

| event | payload | rate |
|---|---|---|
| `meters` | `{ m: [L,R], c: { <id>: [L,R] } }` linear peaks | 60 Hz while the window is visible, nothing otherwise |
| `state` | full `FullState` | on every structural change (100 ms debounce) |
