use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

use tokio::sync::Mutex;

use crate::{
    bluetooth::{detect_rfcomm_channel, resolve_connected_device},
    commands,
    connection::SonyConnection,
    error::SonyError,
    types::{
        AncMode, Battery, DeviceStatus, EqBands, EqPreset, Equalizer, NoiseControl, SessionInfo,
        Toggle,
    },
};

/// The RFCOMM socket reports connected slightly before it can carry data.
const SETTLE_DELAY: Duration = Duration::from_millis(400);

struct Session {
    connection: SonyConnection<bluer::rfcomm::Stream>,
    info: SessionInfo,
}

#[derive(Default)]
pub struct SonyManager {
    session: Mutex<Option<Session>>,
}

impl SonyManager {
    pub fn new() -> Arc<SonyManager> {
        Arc::new(SonyManager::default())
    }

    pub async fn connect(
        &self,
        address: Option<String>,
        channel: Option<u8>,
    ) -> Result<SessionInfo, SonyError> {
        let mut guard = self.session.lock().await;
        if let Some(session) = guard.as_ref() {
            if !session.connection.broken_flag().load(Ordering::SeqCst) {
                return Ok(session.info.clone());
            }
            // A dead session is replaced rather than reported as a conflict.
            *guard = None;
        }

        let device = resolve_connected_device(address).await?;
        let channel = match channel {
            Some(channel) => channel,
            None => detect_rfcomm_channel(&device.address).await?,
        };
        let parsed: bluer::Address = device
            .address
            .parse()
            .map_err(|_| SonyError::Detection(format!("bad address {}", device.address)))?;

        let connection = SonyConnection::open(parsed, channel).await?;
        tokio::time::sleep(SETTLE_DELAY).await;

        let info = SessionInfo {
            address: device.address,
            name: device.name,
            channel,
        };
        *guard = Some(Session {
            connection,
            info: info.clone(),
        });
        Ok(info)
    }

    pub async fn disconnect(&self) -> Result<(), SonyError> {
        self.session.lock().await.take();
        Ok(())
    }

    pub async fn session(&self) -> Option<SessionInfo> {
        let guard = self.session.lock().await;
        guard
            .as_ref()
            .filter(|session| !session.connection.broken_flag().load(Ordering::SeqCst))
            .map(|session| session.info.clone())
    }

    /// Run one request/response exchange, dropping the session if the link died.
    ///
    /// `table` selects the frame data type. Most commands ride the default
    /// table; voice guidance is only answered on the second one.
    async fn exchange_on(
        &self,
        table: crate::protocol::DataType,
        payload: Vec<u8>,
        accepts: &[u8],
    ) -> Result<Vec<u8>, SonyError> {
        let mut guard = self.session.lock().await;
        let session = guard.as_mut().ok_or(SonyError::NoSession)?;

        session.connection.set_data_type(table);

        let result = async {
            session.connection.send(payload).await?;
            // The headset pushes notifications of its own accord, so the next
            // frame is not necessarily the answer to this request. Keep reading
            // until one carries an opcode this command actually expects.
            loop {
                let frame = session.connection.recv().await?;
                match frame.payload.first() {
                    Some(opcode) if accepts.contains(opcode) => return Ok(frame.payload),
                    Some(opcode) => tracing::debug!(
                        "ignoring unrelated frame with opcode {:#04x}",
                        opcode
                    ),
                    None => tracing::debug!("ignoring empty frame"),
                }
            }
        }
        .await;

        if result.is_err() && session.connection.broken_flag().load(Ordering::SeqCst) {
            *guard = None;
        }
        result
    }

    async fn exchange(&self, payload: Vec<u8>, accepts: &[u8]) -> Result<Vec<u8>, SonyError> {
        self.exchange_on(commands::DEFAULT_TABLE, payload, accepts)
            .await
    }

    pub async fn battery(&self) -> Result<Battery, SonyError> {
        let reply = self.exchange(commands::battery_request(), commands::BATTERY_ACCEPTS).await?;
        commands::parse_battery(&reply)
    }

    pub async fn noise_control(&self) -> Result<NoiseControl, SonyError> {
        let reply = self.exchange(commands::noise_control_request(), commands::NOISE_CONTROL_ACCEPTS).await?;
        commands::parse_noise_control(&reply)
    }

    pub async fn set_noise_control(
        &self,
        control: NoiseControl,
    ) -> Result<NoiseControl, SonyError> {
        let payload = commands::set_noise_control(control)?;
        match self.exchange(payload, commands::NOISE_CONTROL_ACCEPTS).await {
            Ok(reply) => commands::parse_noise_control(&reply),
            // Writing values the headset already holds produces no
            // notification, so silence means "applied", not "failed".
            Err(SonyError::Timeout(_)) => self.noise_control().await,
            Err(err) => Err(err),
        }
    }

    /// Switch mode while preserving the ambient level and voice focus the
    /// headset is already set to, so changing mode does not silently reset them.
    pub async fn set_anc_mode(&self, mode: AncMode) -> Result<NoiseControl, SonyError> {
        let current = self.noise_control().await?;
        self.set_noise_control(NoiseControl { mode, ..current }).await
    }

    pub async fn equalizer(&self) -> Result<Equalizer, SonyError> {
        let reply = self.exchange(commands::eq_request(), commands::EQ_ACCEPTS).await?;
        commands::parse_eq(&reply)
    }

    pub async fn set_eq_preset(&self, preset: EqPreset) -> Result<Equalizer, SonyError> {
        match self.exchange(commands::set_eq_preset(preset), commands::EQ_ACCEPTS).await {
            Ok(reply) => commands::parse_eq(&reply),
            Err(SonyError::Timeout(_)) => self.equalizer().await,
            Err(err) => Err(err),
        }
    }

    /// Write band values. Applies them to the active preset when it accepts
    /// them, otherwise to User 1.
    pub async fn set_eq_bands(&self, bands: EqBands) -> Result<Equalizer, SonyError> {
        let current = self.equalizer().await?;
        let preset = if current.preset.is_customizable() {
            current.preset
        } else {
            EqPreset::User1
        };
        match self.exchange(commands::set_eq_bands(preset, bands)?, commands::EQ_ACCEPTS).await {
            Ok(reply) => commands::parse_eq(&reply),
            Err(SonyError::Timeout(_)) => self.equalizer().await,
            Err(err) => Err(err),
        }
    }

    pub async fn dsee(&self) -> Result<Toggle, SonyError> {
        let reply = self.exchange(commands::dsee_request(), commands::DSEE_ACCEPTS).await?;
        commands::parse_dsee(&reply)
    }

    pub async fn set_dsee(&self, enabled: bool) -> Result<Toggle, SonyError> {
        match self.exchange(commands::set_dsee(enabled), commands::DSEE_ACCEPTS).await {
            Ok(reply) => commands::parse_dsee(&reply),
            Err(SonyError::Timeout(_)) => self.dsee().await,
            Err(err) => Err(err),
        }
    }

    pub async fn voice_guidance(&self) -> Result<Toggle, SonyError> {
        let reply = self
            .exchange_on(
                commands::VOICE_GUIDANCE_TABLE,
                commands::voice_guidance_request(),
                commands::VOICE_GUIDANCE_ACCEPTS,
            )
            .await?;
        commands::parse_voice_guidance(&reply)
    }

    pub async fn set_voice_guidance(&self, enabled: bool) -> Result<Toggle, SonyError> {
        match self
            .exchange_on(
                commands::VOICE_GUIDANCE_TABLE,
                commands::set_voice_guidance(enabled),
                commands::VOICE_GUIDANCE_ACCEPTS,
            )
            .await
        {
            Ok(reply) => commands::parse_voice_guidance(&reply),
            Err(SonyError::Timeout(_)) => self.voice_guidance().await,
            Err(err) => Err(err),
        }
    }

    /// Everything the device reports, in one call. A feature the model does
    /// not support is reported as `null` rather than failing the whole call.
    pub async fn status(&self) -> Result<DeviceStatus, SonyError> {
        let session = self.session().await.ok_or(SonyError::NoSession)?;
        Ok(DeviceStatus {
            session,
            battery: optional(self.battery().await)?,
            noise_control: optional(self.noise_control().await)?,
            equalizer: optional(self.equalizer().await)?,
            dsee: optional(self.dsee().await)?,
            voice_guidance: optional(self.voice_guidance().await)?,
        })
    }
}

/// Treat an unsupported or undecodable reading as absent; propagate anything
/// that means the link itself is in trouble.
fn optional<T>(result: Result<T, SonyError>) -> Result<Option<T>, SonyError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(SonyError::Unsupported(_)) | Err(SonyError::InvalidFrame) => Ok(None),
        Err(SonyError::Timeout(_)) => Ok(None),
        Err(err) => Err(err),
    }
}
