use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SonyError {
    #[error("not connected to the headset")]
    NotConnected,
    #[error("no active session")]
    NoSession,
    #[error("a session is already active")]
    AlreadyConnected,
    #[error("timed out while waiting for {0}")]
    Timeout(&'static str),
    #[error("failed to decode frame")]
    InvalidFrame,
    #[error("incorrect frame checksum")]
    ChecksumMismatch,
    #[error("received an ACK when a data frame was expected")]
    UnexpectedAck,
    #[error("operation '{0}' is not supported by the connected model")]
    Unsupported(&'static str),
    #[error("failed to detect device: {0}")]
    Detection(String),
    #[error("command `{command}` failed: {output}")]
    CommandFailed { command: String, output: String },
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}
