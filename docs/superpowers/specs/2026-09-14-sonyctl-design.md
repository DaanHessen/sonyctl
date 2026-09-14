# sonyctl — Sony headphone CLI/API design

Date: 2026-09-14
Status: approved

## Purpose

An unofficial Rust CLI + HTTP API that controls Sony Bluetooth headphones
from Linux, mirroring the shape and ship surface of the existing `earctl`
tool for Nothing earbuds.

Target device: Sony WH-XB910N (`14:3F:A6:DB:86:A9`).

## Why a separate tool

Sony and Nothing share only the RFCOMM transport. Framing, checksums,
sequencing, and every command ID differ. A vendor-abstraction layer inside
earctl would share little beyond `bluer::rfcomm::Stream` while putting a
working tool at risk, so `sonyctl` is a sibling project that borrows
earctl's module layout and conventions rather than its code.

## Transport

Sony exposes a vendor SPP service:

- Service name: `Serial HPC`
- UUID: `956c7b26-d49a-4ba8-b03f-b17d393cb6e2`
- RFCOMM channel: 9 on the WH-XB910N (resolved from SDP at runtime, not
  hardcoded)

The UUID is the device identity check. Bluetooth names are user-renameable,
so detection matches on the advertised service, the same way earctl matches
Nothing's `aeac4a03-…`.

## Protocol

Sony's framed serial protocol, as reverse-engineered by the community
(SonyHeadphonesClient and related projects):

```
0x3e | dataType | seqNumber | payloadLength (u32 BE) | payload | checksum | 0x3c
```

- `checksum` = sum of every byte from `dataType` through the last payload
  byte, mod 256.
- Escaping applies to the frame body and checksum, never the START/END
  markers: a literal `0x3e`, `0x3c`, or `0x3d` is written as `0x3d`
  followed by the byte with bit 4 cleared (`& 0xEF`), giving `0x2e`,
  `0x2c`, `0x2d`.
- `seqNumber` toggles between `0x00` and `0x01`.
- Every DATA frame sent must be acknowledged by the headphones with an ACK
  frame (empty payload, flipped sequence number) before the next DATA frame
  is sent. Every DATA frame received from the headphones must be
  acknowledged by us in the same way.

This ACK state machine is the substantive difference from earctl, whose
protocol is fire-and-read with a CRC16 and no acknowledgement layer.

Command IDs are the first payload byte and vary across Sony protocol
versions. They are NOT hardcoded from memory: see "Build order" below.

## Architecture

Crate `sony_api`, binary `sonyctl`. Module layout mirrors earctl so the two
projects read alike:

| Module | Responsibility |
|---|---|
| `protocol.rs` | Frame encode/decode, escaping, checksum, frame types |
| `connection.rs` | RFCOMM stream, split read/write halves, ACK state machine, broken-link flag |
| `service.rs` | `SonyManager`: session lifecycle, request/response correlation, notification cache |
| `types.rs` | `AncMode`, `AmbientLevel(0..=20)`, `FocusOnVoice`, `EqPreset`, `EqBands`, `Battery`, `DeviceStatus` |
| `bluetooth.rs` | Connected-device discovery, SDP resolution of the Sony service to an RFCOMM channel |
| `server.rs` | axum REST API, route shape matching earctl's |
| `error.rs` | `SonyError` |
| `main.rs` | clap CLI with a global `--endpoint`, subcommands talking to the server |

Each unit is independently testable: `protocol.rs` is pure functions over
byte slices and needs no hardware; `connection.rs` is testable against an
in-memory duplex stream; `service.rs` is testable against a fake connection.

## v1 features

Core set only:

- Battery level and charging state (single battery — no L/R/case split)
- Noise control: Off / Ambient Sound / Noise Cancelling, ambient level
  `0..=20`, focus-on-voice toggle
- Equalizer: presets (off, bright, excited, mellow, relaxed, vocal, treble,
  bass, speech, custom 1, custom 2), 5-band custom EQ plus clear bass,
  each `-10..=10`
- Aggregate `status` returning everything the device reports in one call
- Session management: detect, connect, auto-connect, disconnect, session

Deliberately out of scope for v1: DSEE Extreme, Speak-to-Chat, wearing
detection, auto power-off, touch-sensor assignment, 360 Reality Audio,
sound position, multipoint.

## CLI surface

```
sonyctl server --addr 127.0.0.1:8788
sonyctl detect | connect --address … | auto-connect | disconnect | session
sonyctl status
sonyctl battery
sonyctl anc get | set off|ambient|anc
sonyctl ambient get | set <0-20> [--focus-on-voice true|false]
sonyctl eq get | set <preset>
sonyctl eq bands set --clear-bass <-10..10> --band <hz> <-10..10> …
sonyctl probe …
```

Port 8788 so the server runs beside earctl's 8787.

## Ship surface

Full earctl parity:

- `sonyctl.service` — systemd user unit, same hardening as earctl's
- `contrib/waybar/sonyctl-waybar.sh` — waybar module, ported 1:1 from
  `earctl-waybar.sh`
- `PKGBUILD`, `.SRCINFO`, `.install`, `update-aur.sh`
- `README.md`, `LICENSE`

### Waybar module

Structural port of `earctl-waybar.sh`, keeping: auto-hide unless the device
is connected, `offline` / `connecting` / live states, battery low and
critical classes with one-shot `notify-send`, the walker→wofi `pick()`
menu, and the `●`/`○` selection marks.

Differences, all forced by the device:

- Identity check uses Sony's `956c7b26-…` UUID
- One battery, not left/right/case
- Actions: `anc-cycle` (ANC → Ambient → Off), `ambient <0-20>`,
  `eq <preset>`, `menu`, `reconnect`. No latency, in-ear, bass, or ring —
  the WH-XB910N has no equivalents. Middle-click cycles ambient level.
- Own state dir `$XDG_RUNTIME_DIR/sonyctl-waybar`, own endpoint env
  `SONYCTL_ENDPOINT`, own refresh signal, so both modules coexist.

### Live waybar changes

Applied to `~/.config/waybar/`, with `config.jsonc` and `style.css` backed
up first:

1. Install `sonyctl-waybar.sh` into `scripts/`
2. Add `custom/sonyctl` to `modules-right` beside `custom/earctl`, and its
   module definition
3. Add a `#custom-sonyctl` CSS block matching the existing
   `#custom-earctl` styling
4. Remove `pulseaudio` from `modules-right` (user decision; the module
   definition stays in the file). Trade-off accepted by the user: the bar
   loses scroll-to-change-volume, right-click mute, click-to-open audio
   settings, and the active-sink readout. Keyboard volume keys are
   unaffected.

## Build order

The frame format is well established. The per-command IDs for this
particular device are not, and will not be guessed into a shipped binary.

1. TDD the codec offline — encode, decode, escaping round-trip, checksum,
   sequence toggling, partial and concatenated frames. No hardware.
2. Connect to the live headphones and query protocol/capability info; dump
   what the device reports.
3. Use `sonyctl probe` to send candidate GET opcodes and print decoded
   replies, confirming the battery, noise-control, and EQ command IDs
   against the real device.
4. Implement only confirmed commands, verifying each by setting a value and
   reading it back.
5. Build the server, CLI, packaging, and waybar module on the confirmed
   command set.

## Risks

- **Unknown command IDs.** Mitigated by the probe step; no command ships
  unverified.
- **Testing mutates live device state.** ANC and EQ settings will change
  during probing. All commands are runtime settings that the official Sony
  app also sets; nothing is written to firmware.
- **Single-device verification.** Only the WH-XB910N can be tested here.
  Other Sony models are unclaimed until someone verifies them.
