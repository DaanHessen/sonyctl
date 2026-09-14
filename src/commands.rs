//! Command payloads confirmed against a live WH-XB910N.
//!
//! Every opcode and byte offset here is recorded in `docs/protocol.md` with
//! the reply it was derived from. Nothing is inferred from other models.

use crate::{
    error::SonyError,
    protocol::DataType,
    types::{
        AncMode, Battery, EqBands, EqPreset, Equalizer, NoiseControl, EQ_BAND_MAX, EQ_BAND_MIN,
        EQ_BAND_OFFSET, MAX_AMBIENT_LEVEL,
    },
};
use crate::types::{Toggle, Volume, MAX_VOLUME};

/// Response opcodes each feature may answer with: the return, and the
/// notification a set is acknowledged by. A reply whose first byte is not in
/// this list belongs to some other exchange and must not be parsed as ours.
pub const BATTERY_ACCEPTS: &[u8] = &[0x23, 0x24];
pub const NOISE_CONTROL_ACCEPTS: &[u8] = &[0x67, 0x69];
pub const EQ_ACCEPTS: &[u8] = &[0x57, 0x59];
pub const DSEE_ACCEPTS: &[u8] = &[0xe7, 0xe9];
pub const VOICE_GUIDANCE_ACCEPTS: &[u8] = &[0x47, 0x49];
pub const VOLUME_ACCEPTS: &[u8] = &[0xa7, 0xa9];

/// The table almost every command rides.
pub const DEFAULT_TABLE: DataType = DataType::DataMdr;
/// Voice guidance is only answered on the second table.
pub const VOICE_GUIDANCE_TABLE: DataType = DataType::DataMdrNo2;

// Battery
const BATTERY_GET: u8 = 0x22;
const BATTERY_RET: u8 = 0x23;
const BATTERY_TYPE: u8 = 0x00;

// Noise control. 0x16 is the inquired type this model answers on.
const NCASM_GET: u8 = 0x66;
const NCASM_RET: u8 = 0x67;
const NCASM_SET: u8 = 0x68;
const NCASM_NTFY: u8 = 0x69;
const NCASM_TYPE: u8 = 0x16;
/// `p0`. A set with any other value is ignored by the headset.
const NCASM_ENABLED: u8 = 0x01;
/// `p3`. Selects ambient level adjustment.
const ASM_SETTING_LEVEL: u8 = 0x02;

// Equalizer
const EQ_GET: u8 = 0x56;
const EQ_RET: u8 = 0x57;
/// A set is answered with a notification rather than a return.
const EQ_NTFY: u8 = 0x59;
const EQ_SET: u8 = 0x58;
const EQ_TYPE: u8 = 0x00;
const EQ_BAND_COUNT: u8 = 0x06;

// --- battery -----------------------------------------------------------------

pub fn battery_request() -> Vec<u8> {
    vec![BATTERY_GET, BATTERY_TYPE]
}

pub fn parse_battery(payload: &[u8]) -> Result<Battery, SonyError> {
    if payload.len() < 4 || payload[0] != BATTERY_RET {
        return Err(SonyError::InvalidFrame);
    }
    Ok(Battery {
        percent: payload[2],
        charging: payload[3] != 0,
    })
}

// --- noise control -----------------------------------------------------------

pub fn noise_control_request() -> Vec<u8> {
    vec![NCASM_GET, NCASM_TYPE]
}

pub fn parse_noise_control(payload: &[u8]) -> Result<NoiseControl, SonyError> {
    if payload.len() < 8 {
        return Err(SonyError::InvalidFrame);
    }
    if !matches!(payload[0], NCASM_RET | NCASM_NTFY) || payload[1] != NCASM_TYPE {
        return Err(SonyError::InvalidFrame);
    }

    // p1 is the on/off switch, p2 selects noise cancelling or ambient.
    let mode = match (payload[3], payload[4]) {
        (0x00, _) => AncMode::Off,
        (_, 0x00) => AncMode::NoiseCancelling,
        (_, _) => AncMode::Ambient,
    };

    Ok(NoiseControl {
        mode,
        focus_on_voice: payload[6] != 0,
        ambient_level: payload[7],
    })
}

pub fn set_noise_control(control: NoiseControl) -> Result<Vec<u8>, SonyError> {
    if control.ambient_level > MAX_AMBIENT_LEVEL {
        return Err(SonyError::Unsupported("ambient level above 20"));
    }

    let (enabled, ambient) = match control.mode {
        AncMode::Off => (0x00, 0x00),
        AncMode::NoiseCancelling => (0x01, 0x00),
        AncMode::Ambient => (0x01, 0x01),
    };

    Ok(vec![
        NCASM_SET,
        NCASM_TYPE,
        NCASM_ENABLED,
        enabled,
        ambient,
        ASM_SETTING_LEVEL,
        u8::from(control.focus_on_voice),
        control.ambient_level,
    ])
}

// --- equalizer ---------------------------------------------------------------

pub fn eq_request() -> Vec<u8> {
    vec![EQ_GET, EQ_TYPE]
}

pub fn parse_eq(payload: &[u8]) -> Result<Equalizer, SonyError> {
    if payload.len() < 4 || !matches!(payload[0], EQ_RET | EQ_NTFY) {
        return Err(SonyError::InvalidFrame);
    }
    let band_count = payload[3] as usize;
    let values = payload.get(4..4 + band_count).ok_or(SonyError::InvalidFrame)?;
    if band_count != EQ_BAND_COUNT as usize {
        return Err(SonyError::InvalidFrame);
    }

    let decode = |byte: u8| (byte as i16 - EQ_BAND_OFFSET) as i8;
    Ok(Equalizer {
        preset: EqPreset::from_byte(payload[2]).ok_or(SonyError::InvalidFrame)?,
        bands: EqBands {
            clear_bass: decode(values[0]),
            bands: [
                decode(values[1]),
                decode(values[2]),
                decode(values[3]),
                decode(values[4]),
                decode(values[5]),
            ],
        },
    })
}

/// Select a preset and let the headset supply that preset's own band values.
pub fn set_eq_preset(preset: EqPreset) -> Vec<u8> {
    vec![EQ_SET, EQ_TYPE, preset.to_byte(), 0x00]
}

/// Write band values into a customizable preset.
pub fn set_eq_bands(preset: EqPreset, bands: EqBands) -> Result<Vec<u8>, SonyError> {
    if !preset.is_customizable() {
        return Err(SonyError::Unsupported("band values on a fixed preset"));
    }

    let mut values = Vec::with_capacity(6);
    for value in std::iter::once(bands.clear_bass).chain(bands.bands) {
        if !(EQ_BAND_MIN..=EQ_BAND_MAX).contains(&value) {
            return Err(SonyError::Unsupported("eq band outside -10..=10"));
        }
        values.push((value as i16 + EQ_BAND_OFFSET) as u8);
    }

    let mut payload = vec![EQ_SET, EQ_TYPE, preset.to_byte(), EQ_BAND_COUNT];
    payload.extend(values);
    Ok(payload)
}


// --- DSEE / audio upsampling -------------------------------------------------

const DSEE_GET: u8 = 0xe6;
const DSEE_RET: u8 = 0xe7;
const DSEE_SET: u8 = 0xe8;
const DSEE_NTFY: u8 = 0xe9;
const DSEE_TYPE: u8 = 0x00;

pub fn dsee_request() -> Vec<u8> {
    vec![DSEE_GET, DSEE_TYPE]
}

pub fn parse_dsee(payload: &[u8]) -> Result<Toggle, SonyError> {
    if payload.len() < 3 || !matches!(payload[0], DSEE_RET | DSEE_NTFY) {
        return Err(SonyError::InvalidFrame);
    }
    Ok(Toggle::new(payload[2] != 0))
}

pub fn set_dsee(enabled: bool) -> Vec<u8> {
    vec![DSEE_SET, DSEE_TYPE, u8::from(enabled)]
}

// --- voice guidance ----------------------------------------------------------

const VOICE_GUIDANCE_GET: u8 = 0x46;
const VOICE_GUIDANCE_RET: u8 = 0x47;
const VOICE_GUIDANCE_SET: u8 = 0x48;
const VOICE_GUIDANCE_NTFY: u8 = 0x49;
const VOICE_GUIDANCE_TYPE: u8 = 0x01;
/// Trailing byte the headset expects on a set. Constant in every capture.
const VOICE_GUIDANCE_SET_TAIL: u8 = 0x01;

pub fn voice_guidance_request() -> Vec<u8> {
    vec![VOICE_GUIDANCE_GET, VOICE_GUIDANCE_TYPE]
}

/// The wire value is inverted: `0x00` means enabled.
pub fn parse_voice_guidance(payload: &[u8]) -> Result<Toggle, SonyError> {
    if payload.len() < 3 || !matches!(payload[0], VOICE_GUIDANCE_RET | VOICE_GUIDANCE_NTFY) {
        return Err(SonyError::InvalidFrame);
    }
    Ok(Toggle::new(payload[2] == 0))
}

pub fn set_voice_guidance(enabled: bool) -> Vec<u8> {
    vec![
        VOICE_GUIDANCE_SET,
        VOICE_GUIDANCE_TYPE,
        u8::from(!enabled),
        VOICE_GUIDANCE_SET_TAIL,
    ]
}


// --- volume ------------------------------------------------------------------

const VOLUME_GET: u8 = 0xa6;
const VOLUME_RET: u8 = 0xa7;
const VOLUME_SET: u8 = 0xa8;
const VOLUME_NTFY: u8 = 0xa9;
const VOLUME_TYPE: u8 = 0x20;

pub fn volume_request() -> Vec<u8> {
    vec![VOLUME_GET, VOLUME_TYPE]
}

pub fn parse_volume(payload: &[u8]) -> Result<Volume, SonyError> {
    if payload.len() < 3 || !matches!(payload[0], VOLUME_RET | VOLUME_NTFY) {
        return Err(SonyError::InvalidFrame);
    }
    Ok(Volume { level: payload[2] })
}

pub fn set_volume(level: u8) -> Result<Vec<u8>, SonyError> {
    if level > MAX_VOLUME {
        return Err(SonyError::Unsupported("volume above 30"));
    }
    Ok(vec![VOLUME_SET, VOLUME_TYPE, level])
}
