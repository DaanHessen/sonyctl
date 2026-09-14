use std::time::Duration;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "sonyctl",
    version,
    about = "Control Sony headphones from the CLI or via HTTP"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
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

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn".into()),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
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
            sony_api::probe::probe(
                address,
                parsed,
                Duration::from_millis(timeout_ms),
                data_type,
            )
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
    }
    Ok(())
}
