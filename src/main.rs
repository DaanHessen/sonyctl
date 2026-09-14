use std::{io, net::SocketAddr, time::Duration};

use anyhow::{anyhow, Result};
use clap::{CommandFactory, Parser, Subcommand};
use reqwest::{Client, Method};
use serde::Serialize;
use serde_json::{json, Value};
use sony_api::{ApiState, EqBands, EqPreset, SonyManager};

#[derive(Parser)]
#[command(
    name = "sonyctl",
    version,
    about = "Control Sony headphones from the CLI or via HTTP"
)]
struct Cli {
    #[arg(
        long,
        global = true,
        default_value = "http://127.0.0.1:8788",
        help = "HTTP endpoint for the running API server"
    )]
    endpoint: String,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the HTTP API server.
    Server {
        #[arg(long, default_value = "127.0.0.1:8788")]
        addr: String,
    },
    /// Report the connected Sony device and its RFCOMM channel.
    Detect,
    /// Open a session with a specific device.
    Connect {
        #[arg(long)]
        address: String,
        #[arg(long)]
        channel: Option<u8>,
    },
    /// Open a session with whichever Sony device is connected.
    AutoConnect {
        #[arg(long)]
        address: Option<String>,
    },
    /// Close the session.
    Disconnect,
    /// Show the current session.
    Session,
    /// Everything the headset reports, in one call.
    Status,
    /// Battery level and charging state.
    Battery,
    /// Noise control.
    Anc {
        #[command(subcommand)]
        action: AncCommand,
    },
    /// Ambient sound level.
    Ambient {
        #[command(subcommand)]
        action: AmbientCommand,
    },
    /// DSEE Extreme upscaling.
    Dsee {
        #[command(subcommand)]
        action: ToggleCommand,
    },
    /// Spoken notifications and voice guidance.
    VoiceGuidance {
        #[command(subcommand)]
        action: ToggleCommand,
    },
    /// Equalizer.
    Eq {
        #[command(subcommand)]
        action: EqCommand,
    },
    /// Send raw payloads to the headset and print the replies. Development tool.
    Probe {
        #[arg(long)]
        address: Option<String>,
        /// Payload as hex, repeatable: --payload 0000 --payload 2200
        #[arg(long = "payload", required = true)]
        payloads: Vec<String>,
        #[arg(long, default_value = "3000")]
        timeout_ms: u64,
        /// Frame data type byte: 0c for the v1 command table, 0e for v2.
        #[arg(long, default_value = "0c")]
        data_type: String,
    },
    /// Print a shell completion script.
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Print every frame the headset pushes. Development tool.
    Listen {
        #[arg(long)]
        address: Option<String>,
        #[arg(long, default_value = "30")]
        seconds: u64,
        #[arg(long, default_value = "0c")]
        data_type: String,
    },
}

#[derive(Subcommand)]
enum AncCommand {
    Get,
    /// off, noise_cancelling (alias anc), or ambient.
    Set {
        mode: sony_api::AncMode,
    },
}

#[derive(Subcommand)]
enum AmbientCommand {
    Get,
    /// Ambient level, 0-20. Also switches to ambient mode.
    Set {
        level: u8,
        #[arg(long)]
        focus_on_voice: Option<bool>,
    },
}

#[derive(Subcommand)]
enum ToggleCommand {
    Get,
    Set {
        #[arg(
            value_parser = clap::builder::BoolishValueParser::new(),
            value_name = "true|false",
            action = clap::ArgAction::Set
        )]
        enabled: bool,
    },
}

#[derive(Subcommand)]
enum EqCommand {
    Get,
    Set {
        preset: EqPreset,
    },
    /// Write custom band values, each -10..=10.
    Bands {
        #[arg(long, allow_negative_numbers = true)]
        clear_bass: i8,
        /// Five values for 400Hz, 1kHz, 2.5kHz, 6.3kHz, 16kHz.
        #[arg(long, value_delimiter = ',', allow_negative_numbers = true)]
        bands: Vec<i8>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn".into()),
        )
        .init();

    let cli = Cli::parse();
    let endpoint = cli.endpoint.clone();

    match cli.command {
        Commands::Completions { shell } => {
            let mut command = Cli::command();
            let name = command.get_name().to_string();
            clap_complete::generate(shell, &mut command, name, &mut io::stdout());
        }
        Commands::Server { addr } => {
            let addr: SocketAddr = addr.parse()?;
            let state = ApiState {
                manager: SonyManager::new(),
            };
            sony_api::serve_http(addr, state).await?;
        }
        Commands::Probe {
            address,
            payloads,
            timeout_ms,
            data_type,
        } => {
            let parsed = payloads
                .iter()
                .map(|hex| sony_api::probe::parse_hex(hex))
                .collect::<Result<Vec<_>, _>>()?;
            let data_type = sony_api::protocol::DataType::from_byte(
                sony_api::probe::parse_hex(&data_type)?[0],
            );
            sony_api::probe::probe(address, parsed, Duration::from_millis(timeout_ms), data_type)
                .await?;
        }
        Commands::Listen {
            address,
            seconds,
            data_type,
        } => {
            let data_type = sony_api::protocol::DataType::from_byte(
                sony_api::probe::parse_hex(&data_type)?[0],
            );
            sony_api::probe::listen(address, Duration::from_secs(seconds), data_type).await?;
        }
        Commands::Detect => print(api(&endpoint, Method::GET, "/api/detect", None::<()>).await?),
        Commands::Connect { address, channel } => print(
            api(
                &endpoint,
                Method::POST,
                "/api/session/auto-connect",
                Some(json!({ "address": address, "channel": channel })),
            )
            .await?,
        ),
        Commands::AutoConnect { address } => print(
            api(
                &endpoint,
                Method::POST,
                "/api/session/auto-connect",
                Some(json!({ "address": address })),
            )
            .await?,
        ),
        Commands::Disconnect => {
            api(&endpoint, Method::DELETE, "/api/session", None::<()>).await?;
        }
        Commands::Session => print(api(&endpoint, Method::GET, "/api/session", None::<()>).await?),
        Commands::Status => {
            print(connected(&endpoint, Method::GET, "/api/status", None::<()>).await?)
        }
        Commands::Battery => {
            print(connected(&endpoint, Method::GET, "/api/battery", None::<()>).await?)
        }
        Commands::Anc { action } => match action {
            AncCommand::Get => print(
                connected(&endpoint, Method::GET, "/api/noise-control", None::<()>).await?,
            ),
            AncCommand::Set { mode } => print(
                connected(
                    &endpoint,
                    Method::POST,
                    "/api/noise-control",
                    Some(json!({ "mode": mode })),
                )
                .await?,
            ),
        },
        Commands::Ambient { action } => match action {
            AmbientCommand::Get => print(
                connected(&endpoint, Method::GET, "/api/noise-control", None::<()>).await?,
            ),
            AmbientCommand::Set {
                level,
                focus_on_voice,
            } => print(
                connected(
                    &endpoint,
                    Method::POST,
                    "/api/noise-control",
                    Some(json!({
                        "mode": "ambient",
                        "ambient_level": level,
                        "focus_on_voice": focus_on_voice,
                    })),
                )
                .await?,
            ),
        },
        Commands::Dsee { action } => match action {
            ToggleCommand::Get => {
                print(connected(&endpoint, Method::GET, "/api/dsee", None::<()>).await?)
            }
            ToggleCommand::Set { enabled } => print(
                connected(
                    &endpoint,
                    Method::POST,
                    "/api/dsee",
                    Some(json!({ "enabled": enabled })),
                )
                .await?,
            ),
        },
        Commands::VoiceGuidance { action } => match action {
            ToggleCommand::Get => print(
                connected(&endpoint, Method::GET, "/api/voice-guidance", None::<()>).await?,
            ),
            ToggleCommand::Set { enabled } => print(
                connected(
                    &endpoint,
                    Method::POST,
                    "/api/voice-guidance",
                    Some(json!({ "enabled": enabled })),
                )
                .await?,
            ),
        },
        Commands::Eq { action } => match action {
            EqCommand::Get => print(connected(&endpoint, Method::GET, "/api/eq", None::<()>).await?),
            EqCommand::Set { preset } => print(
                connected(
                    &endpoint,
                    Method::POST,
                    "/api/eq",
                    Some(json!({ "preset": preset })),
                )
                .await?,
            ),
            EqCommand::Bands { clear_bass, bands } => {
                let bands: [i8; 5] = bands
                    .try_into()
                    .map_err(|_| anyhow!("--bands needs exactly five values"))?;
                print(
                    connected(
                        &endpoint,
                        Method::POST,
                        "/api/eq/bands",
                        Some(EqBands { clear_bass, bands }),
                    )
                    .await?,
                )
            }
        },
    }
    Ok(())
}

fn print(value: Value) {
    match serde_json::to_string_pretty(&value) {
        Ok(text) => println!("{}", text),
        Err(_) => println!("{}", value),
    }
}

async fn api<B: Serialize>(
    endpoint: &str,
    method: Method,
    path: &str,
    body: Option<B>,
) -> Result<Value> {
    let url = format!("{}{}", endpoint.trim_end_matches('/'), path);
    let mut request = Client::new().request(method, &url);
    if let Some(body) = body {
        request = request.json(&body);
    }

    let response = request.send().await.map_err(|err| {
        anyhow!(
            "could not reach the sonyctl server at {}: {}\n\n\
             Start it with `systemctl --user start sonyctl`, or run \
             `sonyctl server` in another terminal.\n\
             If you were just running `sonyctl probe` or `sonyctl listen`, the \
             daemon was stopped to free the headset's single RFCOMM link.",
            endpoint,
            err
        )
    })?;

    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);

    if !status.is_success() {
        let message = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or(text.as_str());
        return Err(anyhow!("{}", message));
    }
    Ok(value)
}

/// Like `api`, but opens a session first if none is active, so day-to-day
/// commands work without an explicit connect.
async fn connected<B: Serialize>(
    endpoint: &str,
    method: Method,
    path: &str,
    body: Option<B>,
) -> Result<Value> {
    if api(endpoint, Method::GET, "/api/session", None::<()>)
        .await
        .is_err()
    {
        api(
            endpoint,
            Method::POST,
            "/api/session/auto-connect",
            Some(json!({})),
        )
        .await?;
    }
    api(endpoint, method, path, body).await
}
