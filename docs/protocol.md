# Sony MDR protocol. confirmed commands

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
| `0x0c` | v1. carries battery, EQ, and noise control on this device |
| `0x0e` | v2. carries extended device info on this device |

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

- `percent`. battery level, 0-100. Observed `0x57` = 87.
- `charging`. `0x00` when not charging.

`22 01` returns `23 01 00 00 d8 c8` and `22 02` returns `23 02 00 00`. Neither
carries a usable percentage on this model; only `22 00` is used.

BlueZ's own `Battery Percentage` property is cached at connect time and drifts
from this reading. The value here is the live one.

## Noise control

The inquired type is `0x16`. This byte had to be discovered. The common
`0x00` to `0x05` and `0x12` to `0x14` inquired types return nothing on this
device. The working value came from listening for the notification the headset
emits when its own NC/AMBIENT button is pressed.

| Direction | Payload |
|---|---|
| Get | `66 16` |
| Return | `67 16 <p0> <p1> <p2> <p3> <p4> <p5>` |
| Set | `68 16 <p0> <p1> <p2> <p3> <p4> <p5>` |
| Notify | `69 16 <p0> <p1> <p2> <p3> <p4> <p5>` |

| Byte | Meaning |
|---|---|
| `p0` | Always `0x01`. A set with `0x00` is rejected. the headset does not reply at all. |
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

Level range: `0x00` is clamped by the headset to `0x01`. There is no upper
clamp. `0x15`, `0x16` and even `0xff` are stored and echoed back unchanged.
The Sony app's range is 0 to 20, so sonyctl validates that range itself.
Otherwise the device sits in an undefined state.

### Set behaviour

A set that writes values the headset already holds produces no reply at all:
no return and no notification. Silence after a set therefore means the value
was already applied, not that the write failed, so sonyctl reads the current
state back instead of reporting a timeout.

`p4` (focus on voice) and `p5` (level) are ignored unless `p2` is `0x01`. To
change them while noise control is off, switch to ambient, write them, then
switch back.

## Equalizer

| Direction | Payload |
|---|---|
| Get | `56 00` |
| Return | `57 00 <preset> <bandCount> <bands…>` |
| Preset list | `50 00` → `51 00 06 15 0c 00 10 11 12 13 14 15 16 17 a0 a1 a2` |
| Band frequencies | `5a 00` → `5b 00 06 10 00 01 01 01 90 01 03 e8 01 09 c4 01 18 9c 01 3e 80` |

Observed state: `57 00 a1 06 14 0d 0c 0c 0d 0e`.

- `preset`. see the table below. Observed `0xa1` (User 1).
- `bandCount`. `0x06`: clear bass followed by five bands.
- Each band value is offset by 10. The wire byte `0x00` is -10, `0x0a` is 0,
  and `0x14` is +10. The observed `14 0d 0c 0c 0d 0e` is therefore clear bass
  +10, then +3, +2, +2, +3, +4.

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

## DSEE Extreme (audio upsampling)

| Direction | Payload |
|---|---|
| Get | `e6 00` |
| Return | `e7 00 <0\|1>` |
| Set | `e8 00 <0\|1>` |
| Notify | `e9 00 <0\|1>` |

Confirmed by toggling and reading back. `e6 01` also answers (`e7 01 01`) and
is not decoded.

## Voice guidance

The only command found so far that rides the v2 table (`dataType` `0x0e`).
Sending it on the v1 table returns nothing, which is why an earlier v1-only
sweep missed it.

| Direction | Payload |
|---|---|
| Get | `46 01` |
| Return | `47 01 <value> 01` |
| Set | `48 01 <value> 01` |
| Notify | `49 01 <value>` |

The value is inverted: `0x00` means enabled, `0x01` means disabled.
Confirmed by disabling, reading back, and re-enabling.

Note that enabling this does **not** make the headset announce noise-control
changes made over the protocol. See "Voice prompts" below.

## Voice prompts on mode change

The headset speaks the mode aloud when its own NC/AMBIENT button is pressed,
but not when the mode is set over the protocol. Ruled out so far:

- The fixed byte `p0` is not an "announce" flag. Gadgetbridge hardcodes it to
  `0x01` exactly as we do; it distinguishes a committed value from a slider
  drag in progress.
- Voice guidance is enabled on the device (verified by reading `46 01`).
- The inquired type is not the cause. The reference implementation uses `0x17`
  for wind-noise-capable models and `0x15` otherwise; this device answers only
  on `0x15` and `0x16`, and both are silent on set.

No open-source implementation exposes a "speak now" trigger. The working
theory is that the announcement is a button-press behaviour rather than a
protocol-visible one, but this is **not confirmed**.

## Response correlation

The headset emits notifications unprompted, so the next frame after a request
is not necessarily its answer. Every exchange must match the reply's first
byte against the opcodes that command can answer with, and skip anything else.
Ignoring this produces sporadic decode failures that do not reproduce under
the raw probe.

## Answers but undecoded

These respond with plausible payloads but their fields have not been
confirmed, so nothing in sonyctl reads or writes them:

| Request | Reply | Likely feature (per Gadgetbridge naming, unverified) |
|---|---|---|
| `f6 03` | `f7 03 02 ff ff` | Automatic power off / button mode |
| `f6 04` | `f7 04 30` | " |
| `f6 05` | `f7 05 01` | " |
| `f6 06` | `f7 06 00 00 03 00 00 00` | " |
| `f6 07` | `f7 07 00` | " |
| `f6 09` | `f7 09 00` | " |
| `fa 03` | `fb 03 01 00 01 00 01` | Speak-to-Chat config |
| `fa 06` | `fb 06 00 00 f0 00 00 00` | " |
| `a6 01` | `a7 01 01 00 01 00 01 00 01 00` | Volume |
| `a6 20` | `a7 20 0f` | " |
| `80 01` | `81 01 02 00` | unknown |
| `80 02` | `81 02 22 00` | unknown |

Opcode reference from Gadgetbridge's `PayloadTypeV1`: sound position `0x46`,
equalizer `0x56`, ambient sound control `0x66`, volume `0xa6`, NC optimizer
`0x86`, touch sensor `0xd6`, audio upsampling `0xe6`, automatic power off
`0xf6`, Speak-to-Chat `0xfa`, all on the v1 table; voice notifications `0x46`
on the v2 table. Touch sensor (`0xd6`) does not answer on this model.

## Unconfirmed

These answered a probe but their payloads have not been decoded, and nothing
in sonyctl depends on them:

`52 00`, `62 00`, `70 00`, `74 00`, `78 00`, `80 00`, `82 00`, `86 00`,
`b0 00`, `d2 00`, `e2 00`, `e6 00`, `e8 00`, and `30 00`/`32 00` on table v2.

### Set

| Purpose | Payload |
|---|---|
| Select a preset | `58 00 <preset> 00` |
| Write custom bands | `58 00 <preset> 06 <6 offset bytes>` |

A set is answered with a **notification** (`59 …`), not a return (`57 …`), so
both opcodes decode with the same layout. Band writes are only accepted by
the customizable presets `0xa0`, `0xa1` and `0xa2`.
