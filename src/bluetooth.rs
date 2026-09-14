use tokio::process::Command;

use crate::error::SonyError;

pub const SONY_SPP_UUID: &str = "956c7b26-d49a-4ba8-b03f-b17d393cb6e2";

#[derive(Debug, Clone)]
pub struct BluetoothDevice {
    pub address: String,
    pub name: String,
}

pub async fn list_connected_devices() -> Result<Vec<BluetoothDevice>, SonyError> {
    let output = run_command("bluetoothctl", &["devices", "Connected"]).await?;
    Ok(output
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 3 {
                return None;
            }
            Some(BluetoothDevice {
                address: parts[1].to_string(),
                name: parts[2..].join(" "),
            })
        })
        .collect())
}

/// Identity check. Bluetooth names are user-renameable; the vendor service
/// UUID is not.
pub async fn is_sony_device(address: &str) -> bool {
    match run_command("bluetoothctl", &["info", address]).await {
        Ok(info) => info.to_lowercase().contains(SONY_SPP_UUID),
        Err(_) => false,
    }
}

pub async fn resolve_connected_device(
    preferred_address: Option<String>,
) -> Result<BluetoothDevice, SonyError> {
    let connected = list_connected_devices().await?;

    if let Some(address) = preferred_address {
        return connected
            .into_iter()
            .find(|device| device.address.eq_ignore_ascii_case(&address))
            .ok_or_else(|| {
                SonyError::Detection(format!(
                    "bluetooth device {} is not currently connected",
                    address
                ))
            });
    }

    for device in connected {
        if is_sony_device(&device.address).await {
            return Ok(device);
        }
    }

    Err(SonyError::Detection(
        "no connected Sony headphones were found; connect them first".to_string(),
    ))
}

pub async fn detect_rfcomm_channel(address: &str) -> Result<u8, SonyError> {
    let output = run_command("sdptool", &["browse", address]).await?;
    let mut in_sony_record = false;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Service Name:") {
            in_sony_record = false;
        } else if trimmed.starts_with("UUID 128:") {
            in_sony_record = trimmed.to_lowercase().contains(SONY_SPP_UUID);
        } else if in_sony_record && trimmed.starts_with("Channel:") {
            if let Ok(channel) = trimmed.trim_start_matches("Channel:").trim().parse() {
                return Ok(channel);
            }
        }
    }

    Err(SonyError::Detection(format!(
        "no RFCOMM channel advertising {} was found on {}; pass --channel manually",
        SONY_SPP_UUID, address
    )))
}

async fn run_command(cmd: &str, args: &[&str]) -> Result<String, SonyError> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .await
        .map_err(|err| SonyError::Detection(format!("failed to run `{}`: {}", cmd, err)))?;
    if !output.status.success() {
        return Err(SonyError::CommandFailed {
            command: format!("{} {}", cmd, args.join(" ")),
            output: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
