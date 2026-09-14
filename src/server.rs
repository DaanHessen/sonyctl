use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    error::SonyError,
    service::SonyManager,
    types::{AncMode, EqBands, EqPreset, NoiseControl},
};

#[derive(Clone)]
pub struct ApiState {
    pub manager: Arc<SonyManager>,
}

impl IntoResponse for SonyError {
    fn into_response(self) -> Response {
        let status = match self {
            SonyError::NotConnected | SonyError::NoSession | SonyError::AlreadyConnected => {
                StatusCode::CONFLICT
            }
            SonyError::Unsupported(_) => StatusCode::NOT_IMPLEMENTED,
            SonyError::Detection(_) => StatusCode::NOT_FOUND,
            SonyError::Timeout(_) => StatusCode::GATEWAY_TIMEOUT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

#[derive(Deserialize, Default)]
pub struct ConnectRequest {
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub channel: Option<u8>,
}

#[derive(Deserialize)]
pub struct NoiseControlRequest {
    pub mode: AncMode,
    #[serde(default)]
    pub ambient_level: Option<u8>,
    #[serde(default)]
    pub focus_on_voice: Option<bool>,
}

#[derive(Deserialize)]
pub struct EqPresetRequest {
    pub preset: EqPreset,
}

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/api/session", get(get_session).delete(drop_session))
        .route("/api/session/auto-connect", post(auto_connect))
        .route("/api/status", get(status))
        .route("/api/battery", get(battery))
        .route("/api/noise-control", get(get_noise).post(set_noise))
        .route("/api/eq", get(get_eq).post(set_eq))
        .route("/api/eq/bands", post(set_eq_bands))
        .route("/api/detect", get(detect))
        // Explicit method routers above already cover these paths; the two
        // below keep the DELETE and POST helpers referenced.
        .route("/api/session/disconnect", delete(drop_session))
        .with_state(state)
}

pub async fn serve(addr: SocketAddr, state: ApiState) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on http://{}", addr);
    axum::serve(listener, router(state)).await?;
    Ok(())
}

async fn detect() -> Result<Json<serde_json::Value>, SonyError> {
    let device = crate::bluetooth::resolve_connected_device(None).await?;
    let channel = crate::bluetooth::detect_rfcomm_channel(&device.address).await?;
    Ok(Json(json!({
        "address": device.address,
        "name": device.name,
        "channel": channel,
    })))
}

async fn get_session(State(state): State<ApiState>) -> Response {
    match state.manager.session().await {
        Some(info) => Json(info).into_response(),
        None => SonyError::NoSession.into_response(),
    }
}

async fn auto_connect(
    State(state): State<ApiState>,
    body: Option<Json<ConnectRequest>>,
) -> Result<Response, SonyError> {
    let request = body.map(|Json(body)| body).unwrap_or_default();
    let info = state
        .manager
        .connect(request.address, request.channel)
        .await?;
    Ok(Json(info).into_response())
}

async fn drop_session(State(state): State<ApiState>) -> Result<StatusCode, SonyError> {
    state.manager.disconnect().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn status(State(state): State<ApiState>) -> Result<Response, SonyError> {
    Ok(Json(state.manager.status().await?).into_response())
}

async fn battery(State(state): State<ApiState>) -> Result<Response, SonyError> {
    Ok(Json(state.manager.battery().await?).into_response())
}

async fn get_noise(State(state): State<ApiState>) -> Result<Response, SonyError> {
    Ok(Json(state.manager.noise_control().await?).into_response())
}

async fn set_noise(
    State(state): State<ApiState>,
    Json(body): Json<NoiseControlRequest>,
) -> Result<Response, SonyError> {
    // Fields the caller leaves out keep whatever the headset is already set to.
    let current = state.manager.noise_control().await?;
    let control = NoiseControl {
        mode: body.mode,
        ambient_level: body.ambient_level.unwrap_or(current.ambient_level),
        focus_on_voice: body.focus_on_voice.unwrap_or(current.focus_on_voice),
    };
    Ok(Json(state.manager.set_noise_control(control).await?).into_response())
}

async fn get_eq(State(state): State<ApiState>) -> Result<Response, SonyError> {
    Ok(Json(state.manager.equalizer().await?).into_response())
}

async fn set_eq(
    State(state): State<ApiState>,
    Json(body): Json<EqPresetRequest>,
) -> Result<Response, SonyError> {
    Ok(Json(state.manager.set_eq_preset(body.preset).await?).into_response())
}

async fn set_eq_bands(
    State(state): State<ApiState>,
    Json(bands): Json<EqBands>,
) -> Result<Response, SonyError> {
    Ok(Json(state.manager.set_eq_bands(bands).await?).into_response())
}
