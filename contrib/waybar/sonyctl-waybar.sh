#!/usr/bin/env bash
# sonyctl-waybar - Waybar module for Sony headphones via the sonyctl API.
#
# Usage:
#   sonyctl-waybar.sh                 print module JSON (used as "exec")
#   sonyctl-waybar.sh anc-cycle       noise cancelling -> ambient -> off
#   sonyctl-waybar.sh anc <mode>      off|noise_cancelling|ambient
#   sonyctl-waybar.sh ambient-cycle   ambient level 1 -> 5 -> 10 -> 15 -> 20
#   sonyctl-waybar.sh ambient <0-20>  set the ambient level
#   sonyctl-waybar.sh voice-toggle    toggle focus on voice
#   sonyctl-waybar.sh eq <preset>     off|bright|excited|mellow|relaxed|vocal|
#                                     treble|bass|speech|custom|user1|user2
#   sonyctl-waybar.sh menu            full control menu (walker / wofi)
#   sonyctl-waybar.sh reconnect       drop and re-open the sonyctl session
#
# The module hides itself unless the headphones are connected over Bluetooth.
# It needs a running `sonyctl server` (e.g. `systemctl --user enable --now
# sonyctl`) and connects the session on its own. Requires curl, jq and
# bluetoothctl.
#
# Environment:
#   SONYCTL_ENDPOINT      API base URL (default http://127.0.0.1:8788)
#   SONYCTL_WAYBAR_SIGNAL waybar signal used to refresh the module (default 14)

set -uo pipefail

ENDPOINT="${SONYCTL_ENDPOINT:-http://127.0.0.1:8788}"
SIGNAL="${SONYCTL_WAYBAR_SIGNAL:-14}"
# Sony's vendor SPP service. Identifies the headphones without relying on
# their (renameable) Bluetooth name.
SONY_UUID="956c7b26-d49a-4ba8-b03f-b17d393cb6e2"
STATE_DIR="${XDG_RUNTIME_DIR:-/tmp}/sonyctl-waybar"
LOW_BATTERY=20
CRITICAL_BATTERY=10
mkdir -p "$STATE_DIR"

# Headphone-family glyphs throughout - these are over-ear headphones, not
# earbuds. earctl owns the earbud glyphs (U+F184F and friends).
ICON_ANC="󰋋"       # nf-md-headphones
ICON_AMBIENT="󰋎"   # nf-md-headset, mic boom reads as "letting sound in"
ICON_OFF="󰟎"       # nf-md-headphones-off, slashed
ICON_OFFLINE="󰋍"   # nf-md-headphones-settings, trailing dots read as "waiting"
ICON_CHARGING="󱐋"

# --- helpers ------------------------------------------------------------------

api() { # api METHOD PATH [JSON]
  local method="$1" path="$2" body="${3:-}"
  if [[ -n $body ]]; then
    curl -sf --max-time 8 -X "$method" -H 'Content-Type: application/json' \
      -d "$body" "$ENDPOINT$path"
  else
    curl -sf --max-time 8 -X "$method" "$ENDPOINT$path"
  fi
}

refresh() { pkill -RTMIN+"$SIGNAL" -x waybar 2>/dev/null || true; }

notify() { # notify TITLE BODY
  notify-send -u low -a sonyctl -i audio-headphones \
    -h string:x-canonical-private-synchronous:sonyctl "$1" "$2" 2>/dev/null || true
}

# Address of the connected Sony device, empty if none.
find_headphones() {
  local addr
  while read -r _ addr _; do
    [[ -n $addr ]] || continue
    if bluetoothctl info "$addr" 2>/dev/null | grep -qi "$SONY_UUID"; then
      echo "$addr"
      return
    fi
  done < <(bluetoothctl devices Connected 2>/dev/null)
}

# Open a session for the given address. Throttled and serialized, because
# waybar may run several instances of this script at once.
connect_session() {
  local addr="$1" stamp="$STATE_DIR/last-connect" now
  now=$(date +%s)
  if [[ -f $stamp ]] && (( now - $(<"$stamp") < 20 )); then
    return 1
  fi
  (
    flock -n 9 || exit 1
    echo "$now" >"$stamp"
    api POST /api/session/auto-connect "{\"address\":\"$addr\"}" >/dev/null
  ) 9>"$STATE_DIR/connect.lock"
}

# Current status JSON, connecting the session first when needed.
get_status() {
  local status
  status=$(api GET /api/status) && { echo "$status"; return 0; }
  [[ -n ${1:-} ]] || return 1
  connect_session "$1" || return 1
  api GET /api/status
}

anc_label() {
  case "$1" in
    off) echo "Off" ;;
    noise_cancelling) echo "Noise cancelling" ;;
    ambient) echo "Ambient sound" ;;
    *) echo "${1:-Unknown}" ;;
  esac
}

eq_label() {
  case "$1" in
    off) echo "Off" ;;
    bright) echo "Bright" ;;
    excited) echo "Excited" ;;
    mellow) echo "Mellow" ;;
    relaxed) echo "Relaxed" ;;
    vocal) echo "Vocal" ;;
    treble) echo "Treble boost" ;;
    bass) echo "Bass boost" ;;
    speech) echo "Speech" ;;
    custom) echo "Custom" ;;
    user1) echo "User 1" ;;
    user2) echo "User 2" ;;
    *) echo "${1:-Unknown}" ;;
  esac
}

on_off() { [[ $1 == true ]] && echo "On" || echo "Off"; }

# --- module output ------------------------------------------------------------

print_hidden() { echo '{"text":"","class":"hidden"}'; }

print_module() {
  local headphones status
  headphones=$(find_headphones)
  if [[ -z $headphones ]]; then
    print_hidden
    return
  fi

  if ! curl -s --max-time 2 -o /dev/null "$ENDPOINT/api/session"; then
    jq -cn --arg icon "$ICON_OFFLINE" '{text: $icon, class: "offline",
      tooltip: "<b>Sony headphones connected</b>\nsonyctl server is not running\n\nsystemctl --user enable --now sonyctl"}'
    return
  fi

  if ! status=$(get_status "$headphones"); then
    jq -cn --arg icon "$ICON_OFFLINE" '{text: $icon, class: "connecting",
      tooltip: "<b>Sony headphones connected</b>\nWaiting for the sonyctl session…\n\nRight-click for options"}'
    return
  fi

  local anc eq
  anc=$(jq -r '.noise_control.mode // ""' <<<"$status")
  eq=$(jq -r '.equalizer.preset // ""' <<<"$status")
  [[ $anc != off && -n $anc ]] && echo "$anc" >"$STATE_DIR/last-mode"
  check_low_battery "$status"

  jq -c \
    --arg anc_label "$(anc_label "$anc")" \
    --arg eq_label "$( [[ -n $eq ]] && eq_label "$eq")" \
    --arg i_anc "$ICON_ANC" --arg i_off "$ICON_OFF" \
    --arg i_amb "$ICON_AMBIENT" --arg i_chg "$ICON_CHARGING" \
    --argjson low "$LOW_BATTERY" --argjson crit "$CRITICAL_BATTERY" '
    (.battery.percent) as $pct
    | (.battery.charging // false) as $charging
    | (.noise_control.mode // "") as $anc
    | (if $anc == "off" then $i_off
       elif $anc == "ambient" then $i_amb
       else $i_anc end) as $icon
    | (.session.name // "Sony headphones") as $name
    | {
        text: ($icon + (if $pct != null then " \($pct)%" else "" end)
               + (if $charging then " \($i_chg)" else "" end)),
        percentage: ($pct // 0),
        class: ([
          (if $anc == "off" then "off"
           elif $anc == "ambient" then "ambient"
           elif $anc == "" then empty else "anc" end),
          (if $pct == null then empty
           elif $pct <= $crit then "critical"
           elif $pct <= $low then "low" else empty end),
          (if $charging then "charging" else empty end)
        ]),
        tooltip: ([
          "<b>\($name)</b>",
          (if $pct != null then
             "Battery         \($pct)%" + (if $charging then " \($i_chg)" else "" end)
           else empty end),
          "",
          (if .noise_control != null then "Noise control   \($anc_label)" else empty end),
          (if $anc == "ambient" then
             "Ambient level   \(.noise_control.ambient_level)/20" else empty end),
          (if $anc == "ambient" then
             "Focus on voice  " + (if .noise_control.focus_on_voice then "On" else "Off" end)
           else empty end),
          (if .equalizer != null then "Equalizer       \($eq_label)" else empty end),
          "",
          "<small>Click: cycle noise control · Middle: ambient level · Right: menu</small>"
        ] | join("\n"))
      }' <<<"$status"
}

# One notification per discharge when the battery crosses the low threshold.
check_low_battery() {
  local pct charging flag="$STATE_DIR/low-battery"
  read -r pct charging < <(jq -r '"\(.battery.percent // "-") \(.battery.charging // false)"' <<<"$1")
  [[ $pct == - ]] && return
  if [[ $charging == true ]] || (( pct > LOW_BATTERY )); then
    rm -f "$flag"
  elif [[ ! -f $flag ]]; then
    touch "$flag"
    notify-send -u normal -a sonyctl -i battery-low "Headphones battery low" \
      "${pct}% remaining" 2>/dev/null || true
  fi
}

# --- actions ------------------------------------------------------------------

require_status() {
  local status
  if ! status=$(get_status "$(find_headphones)"); then
    notify "Headphones" "Not connected to sonyctl"
    exit 1
  fi
  echo "$status"
}

set_anc() {
  local mode="$1"
  case "$mode" in
    anc | nc) mode=noise_cancelling ;;
  esac
  if api POST /api/noise-control "{\"mode\":\"$mode\"}" >/dev/null; then
    notify "Noise control" "$(anc_label "$mode")"
  else
    notify "Noise control" "Failed to switch"
  fi
  refresh
}

anc_cycle() {
  local status current next
  status=$(require_status) || exit 1
  current=$(jq -r '.noise_control.mode // ""' <<<"$status")
  case "$current" in
    noise_cancelling) next=ambient ;;
    ambient) next=off ;;
    *) next=noise_cancelling ;;
  esac
  set_anc "$next"
}

set_ambient() {
  local level="$1"
  if api POST /api/noise-control \
    "{\"mode\":\"ambient\",\"ambient_level\":$level}" >/dev/null; then
    notify "Ambient sound" "Level $level/20"
  else
    notify "Ambient sound" "Failed to switch"
  fi
  refresh
}

ambient_cycle() {
  local status current next
  status=$(require_status) || exit 1
  current=$(jq -r '.noise_control.ambient_level // 0' <<<"$status")
  # The headset clamps 0 up to 1, so 1 is the real floor.
  if (( current < 5 )); then next=5
  elif (( current < 10 )); then next=10
  elif (( current < 15 )); then next=15
  elif (( current < 20 )); then next=20
  else next=1
  fi
  set_ambient "$next"
}

voice_toggle() {
  local status current next
  status=$(require_status) || exit 1
  current=$(jq -r '.noise_control.focus_on_voice // false' <<<"$status")
  [[ $current == true ]] && next=false || next=true
  if api POST /api/noise-control \
    "{\"mode\":\"ambient\",\"focus_on_voice\":$next}" >/dev/null; then
    notify "Focus on voice" "$(on_off "$next")"
  else
    notify "Focus on voice" "Failed to switch"
  fi
  refresh
}

set_eq() {
  if api POST /api/eq "{\"preset\":\"$1\"}" >/dev/null; then
    notify "Equalizer" "$(eq_label "$1")"
  else
    notify "Equalizer" "Failed to switch"
  fi
  refresh
}

reconnect() {
  api DELETE /api/session >/dev/null
  rm -f "$STATE_DIR/last-connect"
  local headphones
  headphones=$(find_headphones)
  if [[ -n $headphones ]] && connect_session "$headphones"; then
    notify "Headphones" "Reconnected"
  else
    notify "Headphones" "Reconnect failed"
  fi
  refresh
}

# --- menu ---------------------------------------------------------------------

pick() { # pick PROMPT OPTIONS(newline separated)
  if command -v omarchy-launch-walker >/dev/null; then
    printf '%b' "$2" | omarchy-launch-walker --dmenu --width 340 --minheight 1 \
      --maxheight 630 -p "$1…" 2>/dev/null
  elif command -v walker >/dev/null; then
    printf '%b' "$2" | walker --dmenu -p "$1…" 2>/dev/null
  else
    printf '%b' "$2" | wofi --dmenu --prompt "$1" 2>/dev/null
  fi
}

mark() { [[ $1 == "$2" ]] && echo "●" || echo "○"; }

menu() {
  local status anc eq choice
  status=$(require_status) || exit 1
  anc=$(jq -r '.noise_control.mode // ""' <<<"$status")
  eq=$(jq -r '.equalizer.preset // ""' <<<"$status")

  local items=""
  [[ -n $anc ]] && items+="󱡏  Noise control    $(anc_label "$anc")\n"
  if [[ $anc == ambient ]]; then
    items+="󰋋  Ambient level    $(jq -r '.noise_control.ambient_level' <<<"$status")/20\n"
    items+="󰍬  Focus on voice   $(on_off "$(jq -r '.noise_control.focus_on_voice' <<<"$status")")\n"
  fi
  [[ -n $eq ]] && items+="󰺢  Equalizer        $(eq_label "$eq")\n"
  items+="󰑓  Reconnect"

  choice=$(pick "Headphones" "$items") || exit 0
  case "$choice" in
    *"Noise control"*) menu_anc "$anc" ;;
    *"Ambient level"*) menu_ambient "$status" ;;
    *"Focus on voice"*) voice_toggle ;;
    *Equalizer*) menu_eq "$eq" ;;
    *Reconnect*) reconnect ;;
  esac
}

menu_anc() {
  local modes=(noise_cancelling ambient off)
  local items="" mode choice
  for mode in "${modes[@]}"; do
    items+="$(mark "$1" "$mode")  $(anc_label "$mode")\n"
  done
  choice=$(pick "Noise control" "${items%\\n}") || exit 0
  for mode in "${modes[@]}"; do
    [[ $choice == *"$(anc_label "$mode")" ]] && set_anc "$mode" && return
  done
}

menu_ambient() {
  local current items="" level choice
  current=$(jq -r '.noise_control.ambient_level // 0' <<<"$1")
  for level in 1 5 10 15 20; do
    items+="$(mark "$current" "$level")  Level $level\n"
  done
  choice=$(pick "Ambient level" "${items%\\n}") || exit 0
  case "$choice" in
    *"Level "*) set_ambient "${choice##*Level }" ;;
  esac
}

menu_eq() {
  local presets=(off bright excited mellow relaxed vocal treble bass speech custom user1 user2)
  local items="" preset choice
  for preset in "${presets[@]}"; do
    items+="$(mark "$1" "$preset")  $(eq_label "$preset")\n"
  done
  choice=$(pick "Equalizer" "${items%\\n}") || exit 0
  for preset in "${presets[@]}"; do
    [[ $choice == *"$(eq_label "$preset")" ]] && set_eq "$preset" && return
  done
}

# --- main ---------------------------------------------------------------------

case "${1:-status}" in
  status) print_module ;;
  anc-cycle) anc_cycle ;;
  anc) set_anc "${2:?mode}" ;;
  ambient-cycle) ambient_cycle ;;
  ambient) set_ambient "${2:?level}" ;;
  voice-toggle) voice_toggle ;;
  eq) set_eq "${2:?preset}" ;;
  menu) menu ;;
  reconnect) reconnect ;;
  *) sed -n '2,20p' "$0" >&2; exit 2 ;;
esac
