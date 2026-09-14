# Sony MDR protocol — confirmed commands

Everything below was confirmed against a live device. Nothing here is
inferred from other models or from documentation.

- **Device:** Sony WH-XB910N, `14:3F:A6:DB:86:A9`
- **Model code:** `HP002`
- **Serial:** `0000000002172598`
- **Firmware:** `VGIDLPB0601`
- **Confirmed on:** 2026-09-14
- **Transport:** RFCOMM channel 9, service UUID `956c7b26-d49a-4ba8-b03f-b17d393cb6e2`

## Framing

`0x3e | dataType | seq | payloadLength (u32 BE) | payload | checksum | 0x3c`

Checksum is the sum of `dataType`, `seq`, the four length bytes and the
payload, modulo 256, over unescaped bytes. Reserved bytes `0x3e`, `0x3c` and
`0x3d` inside the body or checksum are written as `0x3d` followed by the byte
with bit 4 cleared. Every data frame is acknowledged in both directions.

Two command tables exist, distinguished by `dataType`:

| dataType | Table |
|---|---|
| `0x0c` | v1 — carries battery, EQ, and noise control on this device |
| `0x0e` | v2 — carries extended device info on this device |

The RFCOMM socket reports connected slightly before it can carry data;
writing immediately returns `ENOTCONN`. A ~400 ms settle delay after connect
avoids this.

## Handshake

| Request | Reply | Meaning |
|---|---|---|
| `00 00` | `01 00 02 00 00 01 00 00` | Protocol info |
| `02 00` | `03 00 02 11` + ASCII `14:3F:A6:DB:86:A9` | Device's own BD address |
| `06 00` | `07 00 17` + 23 `(functionId, version)` pairs | Supported functions |
| `12 00` | `13 00` + JSON `{"formatVer":"BT01","di":"…"}` | Device identifier |
| `4a 00` (table v2) | `4b 00 14 05 48 50 30 30 32 …` | Model `HP002`, serial, firmware list |

Supported function IDs reported by this device:
`10 12 13 14 20 23 24 27 32 50 69 6a 70 90 93 a1 c1 d1 d2 e1 e2 f4 f9`

## Battery

| Request | Reply |
|---|---|
| `22 00` | `23 00 <percent> <charging>` |

- `percent` — battery level, 0-100. Observed `0x57` = 87.
- `charging` — `0x00` when not charging.

`22 01` returns `23 01 00 00 d8 c8` and `22 02` returns `23 02 00 00`. Neither
carries a usable percentage on this model; only `22 00` is used.

BlueZ's own `Battery Percentage` property is cached at connect time and drifts
from this reading — the value here is the live one.

## Noise control

The inquired type is **`0x16`**. This is the byte that had to be discovered:
the common `0x00`-`0x05` and `0x12`-`0x14` inquired types return nothing on
this device, and the type was found by listening for the notification the
headset emits when its own NC/AMBIENT button is pressed.

| Direction | Payload |
|---|---|
| Get | `66 16` |
| Return | `67 16 <p0> <p1> <p2> <p3> <p4> <p5>` |
| Set | `68 16 <p0> <p1> <p2> <p3> <p4> <p5>` |
| Notify | `69 16 <p0> <p1> <p2> <p3> <p4> <p5>` |

| Byte | Meaning |
|---|---|
| `p0` | Always `0x01`. A set with `0x00` is rejected — the headset does not reply at all. |
| `p1` | `0x00` noise control off, `0x01` on. Values above `0x01` are clamped to `0x01`. |
| `p2` | `0x00` noise cancelling, `0x01` ambient sound. Forced to `0x00` whenever `p1` is `0x00`. |
| `p3` | Always `0x02` (ambient level adjustment). |
| `p4` | `0x00` normal, `0x01` focus on voice. Only takes effect when `p2` is `0x01`. |
| `p5` | Ambient level. Only takes effect when `p2` is `0x01`. |

The three reachable modes:

| Mode | Payload |
|---|---|
| Off | `01 00 00 02 00 <level>` |
| Noise cancelling | `01 01 00 02 00 <level>` |
| Ambient sound | `01 01 01 02 <voice> <level>` |

Confirmed by pressing the headset's own NC/AMBIENT button, which emitted
exactly these three states in the order noise cancelling → ambient → off.

**Level range.** `0x00` is clamped by the headset to `0x01`. There is no upper
clamp: `0x15`, `0x16` and even `0xff` are stored and echoed back verbatim.
The Sony app's range is 0-20, so **sonyctl validates `0..=20` itself** — the
device will otherwise happily sit in an undefined state.

## Equalizer

| Direction | Payload |
|---|---|
| Get | `56 00` |
| Return | `57 00 <preset> <bandCount> <bands…>` |
| Preset list | `50 00` → `51 00 06 15 0c 00 10 11 12 13 14 15 16 17 a0 a1 a2` |
| Band frequencies | `5a 00` → `5b 00 06 10 00 01 01 01 90 01 03 e8 01 09 c4 01 18 9c 01 3e 80` |

Observed state: `57 00 a1 06 14 0d 0c 0c 0d 0e`.

- `preset` — see the table below. Observed `0xa1` (User 1).
- `bandCount` — `0x06`: clear bass followed by five bands.
- Each band value is **offset by 10**: the wire byte `0x00` is −10, `0x0a` is
  0, and `0x14` is +10. The observed `14 0d 0c 0c 0d 0e` is therefore clear
  bass +10, then +3, +2, +2, +3, +4.

Band frequencies, from the `5a 00` reply: 400 Hz (`0x0190`), 1 kHz
(`0x03e8`), 2.5 kHz (`0x09c4`), 6.3 kHz (`0x189c`), 16 kHz (`0x3e80`), plus
clear bass.

Preset IDs, from the `50 00` reply:

| ID | Preset |
|---|---|
| `0x00` | Off |
| `0x10` | Bright |
| `0x11` | Excited |
| `0x12` | Mellow |
| `0x13` | Relaxed |
| `0x14` | Vocal |
| `0x15` | Treble |
| `0x16` | Bass |
| `0x17` | Speech |
| `0xa0` | Custom |
| `0xa1` | User 1 |
| `0xa2` | User 2 |

## Unconfirmed

These answered a probe but their payloads have not been decoded, and nothing
in sonyctl depends on them:

`52 00`, `62 00`, `70 00`, `74 00`, `78 00`, `80 00`, `82 00`, `86 00`,
`b0 00`, `d2 00`, `e2 00`, `e6 00`, `e8 00`, and `30 00`/`32 00` on table v2.

The EQ setter (`58 …`) has not yet been confirmed — see Task 8.
