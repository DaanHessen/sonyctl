//! Byte-level tests. Every literal here was recorded from the live
//! WH-XB910N and is documented in docs/protocol.md.

use sony_api::commands::{
    battery_request, eq_request, noise_control_request, parse_battery, parse_eq,
    parse_noise_control, set_eq_bands, set_eq_preset, set_noise_control,
};
use sony_api::types::{AncMode, EqBands, EqPreset, NoiseControl};

// --- battery -----------------------------------------------------------------

#[test]
fn battery_request_matches_the_confirmed_opcode() {
    assert_eq!(battery_request(), vec![0x22, 0x00]);
}

#[test]
fn parses_a_battery_reply() {
    // Recorded: 23 00 57 00
    let battery = parse_battery(&[0x23, 0x00, 0x57, 0x00]).unwrap();
    assert_eq!(battery.percent, 87);
    assert!(!battery.charging);
}

#[test]
fn parses_a_charging_battery_reply() {
    let battery = parse_battery(&[0x23, 0x00, 0x40, 0x01]).unwrap();
    assert_eq!(battery.percent, 64);
    assert!(battery.charging);
}

#[test]
fn rejects_a_short_battery_reply() {
    assert!(parse_battery(&[0x23]).is_err());
}

#[test]
fn rejects_a_battery_reply_with_the_wrong_opcode() {
    assert!(parse_battery(&[0x57, 0x00, 0x57, 0x00]).is_err());
}

// --- noise control -----------------------------------------------------------

#[test]
fn noise_control_request_matches_the_confirmed_opcode() {
    assert_eq!(noise_control_request(), vec![0x66, 0x16]);
}

#[test]
fn parses_noise_control_off() {
    // Recorded: 67 16 01 00 00 02 00 14
    let control = parse_noise_control(&[0x67, 0x16, 0x01, 0x00, 0x00, 0x02, 0x00, 0x14]).unwrap();
    assert_eq!(control.mode, AncMode::Off);
    assert_eq!(control.ambient_level, 20);
    assert!(!control.focus_on_voice);
}

#[test]
fn parses_noise_cancelling() {
    // Recorded: 67 16 01 01 00 02 00 14
    let control = parse_noise_control(&[0x67, 0x16, 0x01, 0x01, 0x00, 0x02, 0x00, 0x14]).unwrap();
    assert_eq!(control.mode, AncMode::NoiseCancelling);
}

#[test]
fn parses_ambient_with_voice_focus() {
    // Recorded: 67 16 01 01 01 02 01 0a
    let control = parse_noise_control(&[0x67, 0x16, 0x01, 0x01, 0x01, 0x02, 0x01, 0x0a]).unwrap();
    assert_eq!(control.mode, AncMode::Ambient);
    assert_eq!(control.ambient_level, 10);
    assert!(control.focus_on_voice);
}

#[test]
fn rejects_a_short_noise_control_reply() {
    assert!(parse_noise_control(&[0x67, 0x16, 0x01]).is_err());
}

#[test]
fn encodes_noise_control_off() {
    let encoded = set_noise_control(NoiseControl {
        mode: AncMode::Off,
        ambient_level: 20,
        focus_on_voice: false,
    })
    .unwrap();
    assert_eq!(encoded, vec![0x68, 0x16, 0x01, 0x00, 0x00, 0x02, 0x00, 0x14]);
}

#[test]
fn encodes_noise_cancelling() {
    let encoded = set_noise_control(NoiseControl {
        mode: AncMode::NoiseCancelling,
        ambient_level: 20,
        focus_on_voice: false,
    })
    .unwrap();
    assert_eq!(encoded, vec![0x68, 0x16, 0x01, 0x01, 0x00, 0x02, 0x00, 0x14]);
}

#[test]
fn encodes_ambient_with_voice_focus() {
    let encoded = set_noise_control(NoiseControl {
        mode: AncMode::Ambient,
        ambient_level: 10,
        focus_on_voice: true,
    })
    .unwrap();
    assert_eq!(encoded, vec![0x68, 0x16, 0x01, 0x01, 0x01, 0x02, 0x01, 0x0a]);
}

#[test]
fn round_trips_noise_control() {
    for control in [
        NoiseControl { mode: AncMode::Off, ambient_level: 20, focus_on_voice: false },
        NoiseControl { mode: AncMode::NoiseCancelling, ambient_level: 5, focus_on_voice: false },
        NoiseControl { mode: AncMode::Ambient, ambient_level: 1, focus_on_voice: true },
    ] {
        let mut encoded = set_noise_control(control).unwrap();
        // The setter and the return share a payload layout; only the opcode differs.
        encoded[0] = 0x67;
        assert_eq!(parse_noise_control(&encoded).unwrap(), control);
    }
}

#[test]
fn rejects_an_out_of_range_ambient_level() {
    // The headset stores anything up to 0xff without clamping, so the range
    // has to be enforced here.
    assert!(set_noise_control(NoiseControl {
        mode: AncMode::Ambient,
        ambient_level: 21,
        focus_on_voice: false,
    })
    .is_err());
}

// --- equalizer ---------------------------------------------------------------

#[test]
fn eq_request_matches_the_confirmed_opcode() {
    assert_eq!(eq_request(), vec![0x56, 0x00]);
}

#[test]
fn parses_an_eq_reply() {
    // Recorded: 57 00 a1 06 14 0d 0c 0c 0d 0e
    let eq = parse_eq(&[0x57, 0x00, 0xa1, 0x06, 0x14, 0x0d, 0x0c, 0x0c, 0x0d, 0x0e]).unwrap();
    assert_eq!(eq.preset, EqPreset::User1);
    // Wire values are offset by 10.
    assert_eq!(eq.bands.clear_bass, 10);
    assert_eq!(eq.bands.bands, [3, 2, 2, 3, 4]);
}

#[test]
fn parses_a_preset_with_flat_bands() {
    // Recorded after selecting Bass: 57 00 16 06 11 0a 0a 0a 0a 0a
    let eq = parse_eq(&[0x57, 0x00, 0x16, 0x06, 0x11, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a]).unwrap();
    assert_eq!(eq.preset, EqPreset::Bass);
    assert_eq!(eq.bands.clear_bass, 7);
    assert_eq!(eq.bands.bands, [0, 0, 0, 0, 0]);
}

#[test]
fn rejects_an_eq_reply_with_too_few_bands() {
    assert!(parse_eq(&[0x57, 0x00, 0xa1, 0x06, 0x14]).is_err());
}

#[test]
fn encodes_a_preset_change() {
    // Recorded: 58 00 16 00 selects Bass and leaves the bands to the preset.
    assert_eq!(set_eq_preset(EqPreset::Bass), vec![0x58, 0x00, 0x16, 0x00]);
}

#[test]
fn encodes_custom_bands() {
    // Recorded: 58 00 a1 06 0a 0a 0a 0a 0a 0a is flat on User 1.
    let encoded = set_eq_bands(
        EqPreset::User1,
        EqBands { clear_bass: 0, bands: [0, 0, 0, 0, 0] },
    )
    .unwrap();
    assert_eq!(
        encoded,
        vec![0x58, 0x00, 0xa1, 0x06, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a]
    );
}

#[test]
fn encodes_the_full_band_range() {
    let encoded = set_eq_bands(
        EqPreset::User1,
        EqBands { clear_bass: -10, bands: [10, -10, 0, 10, -10] },
    )
    .unwrap();
    assert_eq!(
        encoded,
        vec![0x58, 0x00, 0xa1, 0x06, 0x00, 0x14, 0x00, 0x0a, 0x14, 0x00]
    );
}

#[test]
fn rejects_out_of_range_bands() {
    assert!(set_eq_bands(
        EqPreset::User1,
        EqBands { clear_bass: 0, bands: [11, 0, 0, 0, 0] },
    )
    .is_err());
    assert!(set_eq_bands(
        EqPreset::User1,
        EqBands { clear_bass: -11, bands: [0, 0, 0, 0, 0] },
    )
    .is_err());
}

#[test]
fn maps_every_confirmed_preset_id() {
    // Recorded from the 50 00 preset-list reply.
    for (byte, preset) in [
        (0x00, EqPreset::Off),
        (0x10, EqPreset::Bright),
        (0x11, EqPreset::Excited),
        (0x12, EqPreset::Mellow),
        (0x13, EqPreset::Relaxed),
        (0x14, EqPreset::Vocal),
        (0x15, EqPreset::Treble),
        (0x16, EqPreset::Bass),
        (0x17, EqPreset::Speech),
        (0xa0, EqPreset::Custom),
        (0xa1, EqPreset::User1),
        (0xa2, EqPreset::User2),
    ] {
        assert_eq!(EqPreset::from_byte(byte), Some(preset));
        assert_eq!(preset.to_byte(), byte);
    }
}
