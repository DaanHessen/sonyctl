<a id="readme-top"></a>

[![Contributors][contributors-shield]][contributors-url]
[![Forks][forks-shield]][forks-url]
[![Stargazers][stars-shield]][stars-url]
[![Issues][issues-shield]][issues-url]
[![AGPL License][license-shield]][license-url]

<br />
<div align="center">

<h3 align="center">sonyctl</h3>

  <p align="center">
    An unofficial Rust API and CLI for controlling Sony headphones from Linux.
    <br />
    <a href="docs/protocol.md"><strong>Read the protocol notes »</strong></a>
    <br />
    <br />
    <a href="https://github.com/DaanHessen/sonyctl/issues/new?labels=bug">Report Bug</a>
    &middot;
    <a href="https://github.com/DaanHessen/sonyctl/issues/new?labels=enhancement">Request Feature</a>
  </p>
</div>

<details>
  <summary>Table of Contents</summary>
  <ol>
    <li>
      <a href="#about-the-project">About The Project</a>
      <ul>
        <li><a href="#supported-hardware">Supported hardware</a></li>
        <li><a href="#built-with">Built With</a></li>
      </ul>
    </li>
    <li>
      <a href="#getting-started">Getting Started</a>
      <ul>
        <li><a href="#prerequisites">Prerequisites</a></li>
        <li><a href="#installation">Installation</a></li>
      </ul>
    </li>
    <li>
      <a href="#usage">Usage</a>
      <ul>
        <li><a href="#cli">CLI</a></li>
        <li><a href="#http-api">HTTP API</a></li>
        <li><a href="#waybar">Waybar</a></li>
      </ul>
    </li>
    <li><a href="#how-it-works">How It Works</a></li>
    <li><a href="#roadmap">Roadmap</a></li>
    <li><a href="#contributing">Contributing</a></li>
    <li><a href="#license">License</a></li>
    <li><a href="#contact">Contact</a></li>
    <li><a href="#acknowledgments">Acknowledgments</a></li>
  </ol>
</details>

## About The Project

Sony ships no Linux software for its headphones. Every setting beyond volume
lives in a phone app, so on a desktop you get a Bluetooth audio sink and
nothing else.

sonyctl talks to the headphones directly over Sony's vendor RFCOMM serial
protocol. It runs as a small HTTP server with a CLI client on top, which means
you can change noise control from a script, a keybind, a status bar, or another
machine on your network.

It is the Sony counterpart to [earctl][earctl-url], and deliberately mirrors its
shape so the two sit side by side.

Not affiliated with Sony.

### Supported hardware

| Model | Status |
|---|---|
| WH-XB910N | Every command was reverse-engineered and verified on this unit. |
| Other Sony models | Unverified. The framing is shared across the range, but command IDs vary between protocol generations. |

Nothing in this tool is copied from another model's notes or guessed from
documentation. `docs/protocol.md` records each confirmed byte alongside the
reply it came from and the firmware it was read on.

If you own a different Sony model, `sonyctl probe` and `sonyctl listen` are the
tools used to map this one. Expect to find your own values rather than inherit
these: the noise-control inquired type here turned out to be `0x16`, which
matches no published reverse-engineering notes for the range.

### Built With

* [Rust](https://www.rust-lang.org/)
* [bluer](https://crates.io/crates/bluer) for RFCOMM
* [tokio](https://tokio.rs/)
* [axum](https://crates.io/crates/axum)
* [clap](https://crates.io/crates/clap)

## Getting Started

### Prerequisites

* Rust 1.75 or newer
* `bluez` and `bluez-utils`
* `bluez-deprecated-tools`, which provides the `sdptool` used to find the RFCOMM channel
* Headphones already paired in your desktop Bluetooth settings

### Installation

```sh
git clone https://github.com/DaanHessen/sonyctl.git
cd sonyctl
cargo build --release
install -Dm755 target/release/sonyctl ~/.local/bin/sonyctl
```

Run the server in a terminal, or install the systemd user unit:

```sh
sonyctl server --addr 127.0.0.1:8788
```

```sh
install -Dm644 sonyctl.service ~/.config/systemd/user/sonyctl.service
systemctl --user enable --now sonyctl
```

If you install the binary under your home directory rather than `/usr/bin`,
change `ExecStart` to match and relax `ProtectHome` to `read-only`, or systemd
will hide the binary from its own service.

Port 8788 is the default so that earctl keeps 8787 and both can run at once.

## Usage

```sh
sonyctl status
sonyctl anc set noise_cancelling
sonyctl ambient set 12 --focus-on-voice true
sonyctl eq set bass
sonyctl volume set 12
sonyctl dsee set true
```

The CLI opens a session by itself, so no connect step is needed.

### CLI

| Command | Purpose |
|---|---|
| `sonyctl server --addr <addr>` | Run the HTTP API |
| `sonyctl detect` | Show the connected Sony device and its RFCOMM channel |
| `sonyctl connect --address <addr> [--channel <n>]` | Open a session with a specific device |
| `sonyctl auto-connect` | Open a session with whichever Sony device is connected |
| `sonyctl disconnect`, `sonyctl session` | Close or show the session |
| `sonyctl status` | Every reading in one call |
| `sonyctl battery` | Battery level and charging state |
| `sonyctl anc get\|set <mode>` | `off`, `noise_cancelling` (alias `anc`), `ambient` |
| `sonyctl ambient get\|set <0-20> [--focus-on-voice <bool>]` | Ambient level and voice focus |
| `sonyctl eq get\|set <preset>` | `off`, `bright`, `excited`, `mellow`, `relaxed`, `vocal`, `treble`, `bass`, `speech`, `custom`, `user1`, `user2` |
| `sonyctl eq bands --clear-bass <n> --bands <b1,...,b5>` | Custom bands, each between -10 and 10 |
| `sonyctl volume get\|set <0-30>` | Playback volume |
| `sonyctl dsee get\|set <bool>` | DSEE Extreme upscaling |
| `sonyctl voice-guidance get\|set <bool>` | Spoken notifications |
| `sonyctl completions <shell>` | Print a completion script |
| `sonyctl probe --payload <hex>...` | Send raw payloads and print replies |
| `sonyctl listen --seconds <n>` | Print frames the headset pushes |

Changing noise-control mode keeps the ambient level and voice focus the headset
already holds, so switching to ambient and back does not quietly reset them.

### HTTP API

| Method | Path | Body | Returns |
|---|---|---|---|
| GET | `/api/detect` | | Address, name, channel |
| GET | `/api/session` | | Session, or 409 when there is none |
| POST | `/api/session/auto-connect` | `{"address": "..."}`, optional | Session |
| DELETE | `/api/session` | | 204 |
| GET | `/api/status` | | Every reading |
| GET | `/api/battery` | | `{"percent":82,"charging":false}` |
| GET, POST | `/api/noise-control` | `{"mode":"ambient","ambient_level":10,"focus_on_voice":false}` | Noise control |
| GET, POST | `/api/eq` | `{"preset":"bass"}` | Equalizer |
| POST | `/api/eq/bands` | `{"clear_bass":4,"bands":[2,0,-1,3,5]}` | Equalizer |
| GET, POST | `/api/volume` | `{"level":15}` | Volume |
| GET, POST | `/api/dsee` | `{"enabled":true}` | Toggle |
| GET, POST | `/api/voice-guidance` | `{"enabled":true}` | Toggle |

On a POST to `/api/noise-control`, `ambient_level` and `focus_on_voice` are
optional. Anything left out keeps its current value.

### Waybar

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

## How It Works

Sony advertises a vendor serial service, UUID
`956c7b26-d49a-4ba8-b03f-b17d393cb6e2`, on RFCOMM channel 9 of the WH-XB910N.
sonyctl resolves that channel from SDP at runtime and matches the device by
UUID rather than by name, since names are renameable.

Frames look like this:

```
0x3e | dataType | seq | payloadLength (u32 BE) | payload | checksum | 0x3c
```

Both directions acknowledge every data frame, and the sequence number alternates
between 0 and 1. Reserved bytes inside the body are escaped.

Several device behaviours are handled in code because they surprised us first:

* The RFCOMM socket reports connected slightly before it can carry data. Writing
  immediately returns `ENOTCONN`, so there is a settle delay.
* A set that writes values the headset already holds gets no reply at all.
  Silence after a set means the value was already applied.
* A set is answered with a notification opcode rather than the return opcode.
* The headset pushes notifications unprompted, so the next frame after a request
  is not always its answer. Every exchange matches the reply against the opcodes
  that command can produce.
* Ambient level and voice focus are ignored unless the headset is in ambient
  mode. Changing them while noise control is off means switching to ambient,
  writing, then switching back.
* The headset clamps ambient level 0 up to 1 and applies no upper bound at all.
  It will store 255. sonyctl enforces the 0 to 20 range itself.

There is one RFCOMM link to the headphones, so the daemon and the probe tools
cannot both hold it. Stop the service before probing:

```sh
systemctl --user stop sonyctl
```

## Roadmap

- [x] Battery
- [x] Noise control, ambient level, focus on voice
- [x] Equalizer presets and custom bands
- [x] Playback volume
- [x] DSEE Extreme
- [x] Voice guidance on and off
- [x] HTTP API, systemd unit, waybar module
- [ ] Automatic power off (opcode located, fields not yet decoded)
- [ ] Multipoint and 360 Reality Audio
- [ ] Verification on a second Sony model

Button assignment and voice assistant selection answer a read on this model but
reject every write, and Speak-to-Chat is absent from it entirely, so none of
the three is exposed. `docs/protocol.md` records what each returns.

An open question: the headset announces the mode aloud when its own button is
pressed, but stays silent when the mode is set over the protocol. The effect
byte, the inquired type and the voice-guidance setting have all been ruled out.
No open-source implementation exposes a trigger for it. See `docs/protocol.md`.

See the [open issues][issues-url] for the full list.

## Contributing

Reports from other Sony models are the most useful contribution. If you run
`sonyctl probe` or `sonyctl listen` against a model that is not listed above,
open an issue with the output and the model number.

1. Fork the project
2. Create your branch (`git checkout -b feature/thing`)
3. Commit your changes (`git commit -m 'feat: add thing'`)
4. Push the branch (`git push origin feature/thing`)
5. Open a pull request

Every protocol change needs the byte-level evidence it came from, recorded in
`docs/protocol.md`. Guessed opcodes are not merged.

## License

Distributed under the AGPL-3.0-or-later license. See `LICENSE` for more
information.

## Contact

Daan Hessen - [@DaanHessen](https://github.com/DaanHessen)

Project link: [https://github.com/DaanHessen/sonyctl](https://github.com/DaanHessen/sonyctl)

## Acknowledgments

* [Gadgetbridge](https://codeberg.org/Freeyourgadget/Gadgetbridge) for the most complete public notes on Sony's protocol
* [SonyHeadphonesClient](https://github.com/Plutoberth/SonyHeadphonesClient) for the original framing work
* [sony-device-center](https://github.com/Cyrus7/sony-device-center), which ships a binary by the same name and is unrelated to this project
* [Best-README-Template](https://github.com/othneildrew/Best-README-Template)

<p align="right">(<a href="#readme-top">back to top</a>)</p>

[contributors-shield]: https://img.shields.io/github/contributors/DaanHessen/sonyctl.svg?style=for-the-badge
[contributors-url]: https://github.com/DaanHessen/sonyctl/graphs/contributors
[forks-shield]: https://img.shields.io/github/forks/DaanHessen/sonyctl.svg?style=for-the-badge
[forks-url]: https://github.com/DaanHessen/sonyctl/network/members
[stars-shield]: https://img.shields.io/github/stars/DaanHessen/sonyctl.svg?style=for-the-badge
[stars-url]: https://github.com/DaanHessen/sonyctl/stargazers
[issues-shield]: https://img.shields.io/github/issues/DaanHessen/sonyctl.svg?style=for-the-badge
[issues-url]: https://github.com/DaanHessen/sonyctl/issues
[license-shield]: https://img.shields.io/github/license/DaanHessen/sonyctl.svg?style=for-the-badge
[license-url]: https://github.com/DaanHessen/sonyctl/blob/master/LICENSE
[earctl-url]: https://github.com/DaanHessen/earctl
