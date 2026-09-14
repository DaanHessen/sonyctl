# sonyctl

An unofficial Rust API and CLI for controlling Sony Bluetooth headphones from
Linux — noise control, equalizer, and battery — over Sony's vendor RFCOMM
serial protocol.

It is the Sony counterpart to [earctl](https://github.com/DaanHessen/earctl),
and follows the same shape: one binary that is both an HTTP API server and a
CLI client, a systemd user service, and a waybar module.

Not affiliated with, endorsed by, or supported by Sony.

## Supported hardware

| Model | Status |
|---|---|
| WH-XB910N | Confirmed. Every command was reverse-engineered and verified on this model. |
| Other Sony models | Unverified. The framing is shared across the range, but command IDs vary by protocol generation — see below. |

`docs/protocol.md` records every confirmed byte, the reply it came from, and
the firmware it was confirmed on. Nothing in this tool is inferred from other
models or from documentation.

If you have a different Sony model, `sonyctl probe` and `sonyctl listen` are
the tools used to map it. The noise-control inquired type on the WH-XB910N
turned out to be `0x16`, which is not what the common reverse-engineering
notes suggest, so expect to have to find yours rather than assume it.

## Prerequisites

- Rust 1.75+
- `bluez`, `bluez-utils`
- `bluez-deprecated-tools` (provides `sdptool`, used to find the RFCOMM channel)
- Headphones already paired in your desktop Bluetooth settings

## Install

```sh
git clone https://github.com/DaanHessen/sonyctl.git
cd sonyctl
cargo build --release
```

Then run the server, or install the systemd user unit:

```sh
./target/release/sonyctl server --addr 127.0.0.1:8788
# or
systemctl --user enable --now sonyctl
```

The server listens on port 8788, leaving 8787 to earctl so both can run at once.

## Quick start

```sh
sonyctl status                        # everything, in one call
sonyctl battery
sonyctl anc set noise_cancelling      # or: anc, ambient, off
sonyctl ambient set 12 --focus-on-voice true
sonyctl eq set bass
sonyctl eq bands --clear-bass 4 --bands 2,0,-1,3,5
```

The CLI opens a session on its own, so there is no need to connect first.

## CLI

| Command | Purpose |
|---|---|
| `sonyctl server --addr <addr>` | Run the HTTP API |
| `sonyctl detect` | Show the connected Sony device and its RFCOMM channel |
| `sonyctl connect --address <addr> [--channel <n>]` | Open a session with a specific device |
| `sonyctl auto-connect` | Open a session with whichever Sony device is connected |
| `sonyctl disconnect` / `sonyctl session` | Close / show the session |
| `sonyctl status` | Battery, noise control, and equalizer in one call |
| `sonyctl battery` | Battery level and charging state |
| `sonyctl anc get\|set <mode>` | `off`, `noise_cancelling` (alias `anc`), `ambient` |
| `sonyctl ambient get\|set <0-20> [--focus-on-voice <bool>]` | Ambient level and voice focus |
| `sonyctl eq get\|set <preset>` | `off`, `bright`, `excited`, `mellow`, `relaxed`, `vocal`, `treble`, `bass`, `speech`, `custom`, `user1`, `user2` |
| `sonyctl eq bands --clear-bass <n> --bands <b1,…,b5>` | Custom bands, each `-10..=10` |
| `sonyctl probe --payload <hex>…` | Send raw payloads and print replies (development) |
| `sonyctl listen --seconds <n>` | Print frames the headset pushes (development) |

Switching noise-control mode preserves the ambient level and voice focus the
headset already holds, so changing mode does not silently reset them.

## HTTP API

| Method | Path | Body | Returns |
|---|---|---|---|
| GET | `/api/detect` | — | Address, name, channel |
| GET | `/api/session` | — | Session, or 409 if none |
| POST | `/api/session/auto-connect` | `{"address": "…"}` (optional) | Session |
| DELETE | `/api/session` | — | 204 |
| GET | `/api/status` | — | Battery, noise control, equalizer |
| GET | `/api/battery` | — | `{"percent":82,"charging":false}` |
| GET/POST | `/api/noise-control` | `{"mode":"ambient","ambient_level":10,"focus_on_voice":false}` | Noise control |
| GET/POST | `/api/eq` | `{"preset":"bass"}` | Equalizer |
| POST | `/api/eq/bands` | `{"clear_bass":4,"bands":[2,0,-1,3,5]}` | Equalizer |

On POST to `/api/noise-control`, `ambient_level` and `focus_on_voice` are
optional; anything left out keeps its current value.

## Waybar

`contrib/waybar/sonyctl-waybar.sh` hides itself unless the headphones are
connected. Left-click cycles noise control, middle-click cycles the ambient
level, right-click opens a menu.

```jsonc
"custom/sonyctl": {
  "exec": "~/.config/waybar/scripts/sonyctl-waybar.sh",
  "return-type": "json",
  "interval": 15,
  "signal": 14,
  "on-click": "~/.config/waybar/scripts/sonyctl-waybar.sh anc-cycle",
  "on-click-middle": "~/.config/waybar/scripts/sonyctl-waybar.sh ambient-cycle",
  "on-click-right": "~/.config/waybar/scripts/sonyctl-waybar.sh menu"
}
```

It reads `SONYCTL_ENDPOINT` (default `http://127.0.0.1:8788`) and
`SONYCTL_WAYBAR_SIGNAL` (default 14), so it coexists with the earctl module.

## Known device quirks

Found while verifying against real hardware, and handled in the code:

- The RFCOMM socket reports connected slightly before it can carry data.
  Writing immediately returns `ENOTCONN`, so there is a settle delay.
- A set that writes values the headset already holds gets **no reply at all**.
  Silence after a set means "already applied", not "failed".
- A set is answered with a notification opcode (`0x59`, `0x69`), not the
  return opcode (`0x57`, `0x67`).
- Ambient level and focus-on-voice are ignored unless the headset is in
  ambient mode. To change them while noise control is off, switch to ambient,
  write them, then switch back.
- The headset clamps ambient level 0 up to 1 but applies **no upper bound** —
  it will happily store 255. `sonyctl` enforces `0..=20` itself.
- BlueZ's own battery percentage is cached at connect time and drifts from
  the live reading.

## Naming

The unrelated C++ project
[sony-device-center](https://github.com/Cyrus7/sony-device-center) also ships
a binary called `sonyctl`. Rename this one before publishing to the AUR.

## Packaging

`PKGBUILD` follows the earctl pattern and builds from a release tarball. It
has not been built yet — there is no tagged release, so `sha256sums` is
`SKIP`. Set a real checksum and run `makepkg --printsrcinfo > .SRCINFO`
before publishing.

## License

AGPL-3.0-or-later. See `LICENSE`.
