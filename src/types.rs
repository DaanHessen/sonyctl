use serde::{Deserialize, Serialize};

/// Ambient sound level accepted by the headset. The device clamps 0 up to 1
/// but applies no upper bound at all, so the ceiling is enforced here.
pub const MAX_AMBIENT_LEVEL: u8 = 20;

/// Equalizer band values run -10..=10 and travel the wire offset by this.
pub const EQ_BAND_OFFSET: i16 = 10;
pub const EQ_BAND_MIN: i8 = -10;
pub const EQ_BAND_MAX: i8 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Battery {
    pub percent: u8,
    pub charging: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
#[clap(rename_all = "snake_case")]
pub enum AncMode {
    Off,
    #[clap(alias = "anc")]
    NoiseCancelling,
    Ambient,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoiseControl {
    pub mode: AncMode,
    /// Only meaningful in `Ambient` mode.
    pub ambient_level: u8,
    /// Only meaningful in `Ambient` mode.
    pub focus_on_voice: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
#[clap(rename_all = "snake_case")]
pub enum EqPreset {
    Off,
    Bright,
    Excited,
    Mellow,
    Relaxed,
    Vocal,
    Treble,
    Bass,
    Speech,
    Custom,
    User1,
    User2,
}

impl EqPreset {
    pub fn to_byte(self) -> u8 {
        match self {
            EqPreset::Off => 0x00,
            EqPreset::Bright => 0x10,
            EqPreset::Excited => 0x11,
            EqPreset::Mellow => 0x12,
            EqPreset::Relaxed => 0x13,
            EqPreset::Vocal => 0x14,
            EqPreset::Treble => 0x15,
            EqPreset::Bass => 0x16,
            EqPreset::Speech => 0x17,
            EqPreset::Custom => 0xa0,
            EqPreset::User1 => 0xa1,
            EqPreset::User2 => 0xa2,
        }
    }

    pub fn from_byte(byte: u8) -> Option<EqPreset> {
        Some(match byte {
            0x00 => EqPreset::Off,
            0x10 => EqPreset::Bright,
            0x11 => EqPreset::Excited,
            0x12 => EqPreset::Mellow,
            0x13 => EqPreset::Relaxed,
            0x14 => EqPreset::Vocal,
            0x15 => EqPreset::Treble,
            0x16 => EqPreset::Bass,
            0x17 => EqPreset::Speech,
            0xa0 => EqPreset::Custom,
            0xa1 => EqPreset::User1,
            0xa2 => EqPreset::User2,
            _ => return None,
        })
    }

    /// Whether band values can be written to this preset.
    pub fn is_customizable(self) -> bool {
        matches!(self, EqPreset::Custom | EqPreset::User1 | EqPreset::User2)
    }
}

/// Clear bass plus the five bands at 400 Hz, 1 kHz, 2.5 kHz, 6.3 kHz, 16 kHz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EqBands {
    pub clear_bass: i8,
    pub bands: [i8; 5],
}

pub const EQ_BAND_FREQUENCIES_HZ: [u16; 5] = [400, 1000, 2500, 6300, 16000];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Equalizer {
    pub preset: EqPreset,
    pub bands: EqBands,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub address: String,
    pub name: String,
    pub channel: u8,
}

/// Identity the headset reports about itself, distinct from the Bluetooth
/// name, which the user can change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Sony's internal model code, for example `HP002`.
    pub model_code: String,
    pub serial: String,
    pub device_id: String,
    /// Firmware component versions, in the order the headset lists them.
    pub firmware: Vec<String>,
    /// Marketing model name, for example `WH-XB910N`. Read separately.
    pub model_name: Option<String>,
}

/// Playback volume as the headset reports it. Confirmed range on the
/// WH-XB910N is 0 to 30; the device clamps anything higher down to 30.
pub const MAX_VOLUME: u8 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Volume {
    pub level: u8,
}

/// A plain on/off setting. Used by every boolean feature so they all present
/// the same shape over HTTP and on the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Toggle {
    pub enabled: bool,
}

impl Toggle {
    pub fn new(enabled: bool) -> Toggle {
        Toggle { enabled }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStatus {
    pub session: SessionInfo,
    pub battery: Option<Battery>,
    pub noise_control: Option<NoiseControl>,
    pub equalizer: Option<Equalizer>,
    /// DSEE Extreme / audio upsampling.
    pub dsee: Option<Toggle>,
    /// Spoken notifications and voice guidance.
    pub voice_guidance: Option<Toggle>,
    pub volume: Option<Volume>,
}
