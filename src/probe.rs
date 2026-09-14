//! Development tool: send raw payloads to the headset and print the replies.
//!
//! Command IDs vary between Sony protocol generations, so they are confirmed
//! against the real device here and recorded in `docs/protocol.md` before any
//! of them is wired into a shipped command.

use std::time::Duration;

use crate::{
    bluetooth::{detect_rfcomm_channel, resolve_connected_device},
    connection::SonyConnection,
    error::SonyError,
};

pub fn format_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_ascii(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| {
            if byte.is_ascii_graphic() || *byte == b' ' {
                *byte as char
            } else {
                '.'
            }
        })
        .collect()
}

async fn open(
    address: Option<String>,
) -> Result<(SonyConnection<bluer::rfcomm::Stream>, String, u8), SonyError> {
    let device = resolve_connected_device(address).await?;
    let channel = detect_rfcomm_channel(&device.address).await?;
    let parsed: bluer::Address = device
        .address
        .parse()
        .map_err(|_| SonyError::Detection(format!("bad address {}", device.address)))?;
    let conn = SonyConnection::open(parsed, channel).await?;
    // The RFCOMM socket reports connected slightly before it can carry data;
    // writing immediately yields ENOTCONN. Let it settle.
    tokio::time::sleep(Duration::from_millis(400)).await;
    Ok((conn, format!("{} ({})", device.name, device.address), channel))
}

/// Send each payload in turn and print the reply.
///
/// A timeout means the headset simply did not answer that opcode; the link is
/// still good, so the sweep continues on the same socket. Only an I/O failure
/// reopens the connection. Stale frames left by an earlier timeout are drained
/// and printed separately so they are never mistaken for the current reply.
pub async fn probe(
    address: Option<String>,
    payloads: Vec<Vec<u8>>,
    timeout: Duration,
    data_type: crate::protocol::DataType,
) -> Result<(), SonyError> {
    let (mut conn, label, channel) = open(address.clone()).await?;
    conn.set_timeout(timeout);
    conn.set_data_type(data_type);
    println!("connected to {} on channel {}", label, channel);

    for payload in payloads {
        // Anything still queued belongs to a previous opcode, not this one.
        conn.set_timeout(Duration::from_millis(150));
        while let Ok(frame) = conn.recv().await {
            println!("   late: {}", format_hex(&frame.payload));
        }
        conn.set_timeout(timeout);

        print!("-> {:<12}", format_hex(&payload));
        match conn.request(payload).await {
            Ok(frame) => println!(
                "<- {:<40} |{}|",
                format_hex(&frame.payload),
                format_ascii(&frame.payload)
            ),
            Err(SonyError::Timeout(_)) => println!("<- (no reply)"),
            Err(err) => {
                println!("<- error: {}", err);
                tokio::time::sleep(Duration::from_millis(2000)).await;
                match open(address.clone()).await {
                    Ok((fresh, _, _)) => {
                        conn = fresh;
                        conn.set_timeout(timeout);
                        conn.set_data_type(data_type);
                    }
                    Err(reopen_err) => {
                        println!("   reconnect failed: {}", reopen_err);
                        return Err(reopen_err);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Parse a hex string such as "2200" or "22 00" into bytes.
pub fn parse_hex(input: &str) -> Result<Vec<u8>, SonyError> {
    let cleaned: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.len() % 2 != 0 {
        return Err(SonyError::Detection(format!(
            "hex payload '{}' has an odd number of digits",
            input
        )));
    }
    (0..cleaned.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&cleaned[i..i + 2], 16).map_err(|_| {
                SonyError::Detection(format!("hex payload '{}' is not valid hex", input))
            })
        })
        .collect()
}

/// Print every frame the headset pushes, until the duration elapses.
///
/// Used to discover notification opcodes: change a setting on the headset
/// itself and watch which frame it emits.
pub async fn listen(
    address: Option<String>,
    duration: Duration,
    data_type: crate::protocol::DataType,
) -> Result<(), SonyError> {
    let (mut conn, label, channel) = open(address).await?;
    conn.set_data_type(data_type);
    conn.set_timeout(Duration::from_millis(500));
    println!("listening on {} channel {} for {:?}", label, channel, duration);

    let start = tokio::time::Instant::now();
    let deadline = start + duration;
    while tokio::time::Instant::now() < deadline {
        match conn.recv().await {
            // Elapsed seconds let a capture be lined up against a script of
            // physical actions performed on the headset.
            Ok(frame) => println!(
                "  t+{:>5.1}s  {:02x} | {:<32} |{}|",
                start.elapsed().as_secs_f32(),
                frame.data_type.to_byte(),
                format_hex(&frame.payload),
                format_ascii(&frame.payload)
            ),
            Err(SonyError::Timeout(_)) => continue,
            Err(err) => return Err(err),
        }
    }
    println!("done");
    Ok(())
}
