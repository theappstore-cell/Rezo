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
- **Loss**: if `seq` jumps by 2 and the packet carries the previous frame, decode it then the current one; otherwise use Opus PLC (`decode(None)`). Reference implementation: `tools/rezo-recv/src/main.rs`.
- Recommended receiver buffer: 4 to 15 ms (20 ms by default in the reference receiver).

## 2. Sonos / UPnP (`network/sonos_upnp.rs`)

- Discovery: SSDP `M-SEARCH` (MX 1, sent twice 300 ms apart) for `urn:schemas-upnp-org:device:ZonePlayer:1` and `MediaRenderer:1`, from one socket per private IPv4 interface of the LAN adapter, followed by a GET of the device description (`roomName`, `modelName`, `UDN`, AVTransport `controlURL`).
- Groups: `ZoneGroupTopology#GetZoneGroupState` gives visible and hidden members (stereo pairs, sub, surrounds), the group and its coordinator.
- Stream: `GET http://<pc-ip>:<port>/stream/<id>.wav` returns 16-bit 48 kHz stereo PCM with a `0xFFFFFFFF`-sized WAV header and an endless chunked body, 200 ms of silence first, then live audio.
  - `HEAD` answers with headers only and never touches the active stream.
  - A `Range` request far past the start (tag lookup) gets `416` and leaves the active stream alone.
  - A `Range` request within the first gigabyte is treated as a resume: `206`, live PCM aligned on 4-byte frames, replacing the previous connection.
  - A new full `GET` replaces the previous connection (generation counter).
- Synchronized start: phase 1 in parallel `[BecomeCoordinatorOfStandaloneGroup]`, `Stop`, `SetAVTransportURI` (DIDL-Lite metadata); phase 2 in parallel `Play`.
- Transport events (GENA `SUBSCRIBE`) report stops; if the renderer still holds Rezo's URL a single `Play` resumes it. Polling (`GetTransportInfo` + `GetMediaInfo`) is used when events are unavailable.

## 3. Web receiver (`network/web_receiver.rs`, `web/receiver.html`, `web/worklet.js`)

- `GET /r`: page; `GET /r/worklet.js`: AudioWorklet; `GET /ws?id=<client id>&name=<name>`: WebSocket.
- Server to client: text `{"t":"hello","rate":48000,"ch":2}`, then **binary** messages of 960 bytes (240 interleaved stereo Int16 little-endian frames, 5 ms).
- Client to server: text `{"t":"stats","buf":<buffered ms>}`, used to display the estimated latency.
- Each browser becomes a speaker (`web-<id>`), remembered and enabled automatically on its first connection.
- The worklet runs an adaptive jitter buffer (30 to 160 ms depending on underruns), linear resampling when the audio context is not at 48 kHz, and a ±0.3 % playback-rate servo.

## 4. Tauri events

| event | payload | rate |
|---|---|---|
| `meters` | `{ m: [L,R], c: { <id>: [L,R] } }` linear peaks | 60 Hz while the window is visible, nothing otherwise |
| `state` | full `FullState` | on every structural change (100 ms debounce) |
