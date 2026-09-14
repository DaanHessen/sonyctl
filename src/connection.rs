use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    time,
};

use crate::{
    error::SonyError,
    protocol::{decode_frame, DataType, Frame},
};

const READ_CHUNK: usize = 512;
const DEFAULT_TIMEOUT_MS: u64 = 3000;

pub struct SonyConnection<S> {
    stream: S,
    buffer: Vec<u8>,
    /// Sequence number for the next frame we send. Toggles 0/1 per send.
    tx_seq: u8,
    /// Frame data type used for outbound commands. Sony's v1 command table
    /// rides `DataMdr`; the v2 table rides `DataMdrNo2`.
    data_type: DataType,
    timeout: Duration,
    broken: Arc<AtomicBool>,
}

impl<S> SonyConnection<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    pub fn new(stream: S) -> Self {
        Self {
            stream,
            buffer: Vec::with_capacity(READ_CHUNK),
            tx_seq: 0,
            data_type: DataType::DataMdr,
            timeout: Duration::from_millis(DEFAULT_TIMEOUT_MS),
            broken: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn set_data_type(&mut self, data_type: DataType) {
        self.data_type = data_type;
    }

    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// Flips to `true` once the link has failed, so a session can be dropped
    /// without taking the connection lock.
    pub fn broken_flag(&self) -> Arc<AtomicBool> {
        self.broken.clone()
    }

    /// Send a data frame and wait for the headset to acknowledge it.
    pub async fn send(&mut self, payload: Vec<u8>) -> Result<(), SonyError> {
        let frame = Frame::new(self.data_type, self.tx_seq, payload);
        self.write_frame(&frame).await?;

        let deadline = time::Instant::now() + self.timeout;
        loop {
            let frame = self.read_frame_until(deadline).await?;
            if frame.is_ack() {
                self.tx_seq ^= 1;
                return Ok(());
            }
            // The headset can push a notification while we wait for our ACK.
            // Acknowledge it and keep waiting.
            self.ack(frame.seq).await?;
        }
    }

    /// Return the next data frame, acknowledging it. Stray ACKs are skipped.
    pub async fn recv(&mut self) -> Result<Frame, SonyError> {
        let deadline = time::Instant::now() + self.timeout;
        loop {
            let frame = self.read_frame_until(deadline).await?;
            if frame.is_ack() {
                continue;
            }
            self.ack(frame.seq).await?;
            return Ok(frame);
        }
    }

    pub async fn request(&mut self, payload: Vec<u8>) -> Result<Frame, SonyError> {
        self.send(payload).await?;
        self.recv().await
    }

    async fn ack(&mut self, received_seq: u8) -> Result<(), SonyError> {
        self.write_frame(&Frame::ack(received_seq ^ 1)).await
    }

    async fn write_frame(&mut self, frame: &Frame) -> Result<(), SonyError> {
        let encoded = frame.encode();
        match self.stream.write_all(&encoded).await {
            Ok(()) => {}
            Err(err) => {
                self.broken.store(true, Ordering::SeqCst);
                return Err(SonyError::Io(err));
            }
        }
        match self.stream.flush().await {
            Ok(()) => Ok(()),
            Err(err) => {
                self.broken.store(true, Ordering::SeqCst);
                Err(SonyError::Io(err))
            }
        }
    }

    async fn read_frame_until(&mut self, deadline: time::Instant) -> Result<Frame, SonyError> {
        loop {
            if let Some(frame) = decode_frame(&mut self.buffer)? {
                return Ok(frame);
            }

            let mut chunk = [0u8; READ_CHUNK];
            let read = time::timeout_at(deadline, self.stream.read(&mut chunk))
                .await
                .map_err(|_| SonyError::Timeout("frame"))?;

            match read {
                Ok(0) => {
                    self.broken.store(true, Ordering::SeqCst);
                    return Err(SonyError::NotConnected);
                }
                Ok(n) => self.buffer.extend_from_slice(&chunk[..n]),
                Err(err) => {
                    self.broken.store(true, Ordering::SeqCst);
                    return Err(SonyError::Io(err));
                }
            }
        }
    }
}

impl SonyConnection<bluer::rfcomm::Stream> {
    pub async fn open(address: bluer::Address, channel: u8) -> Result<Self, SonyError> {
        let socket_addr = bluer::rfcomm::SocketAddr::new(address, channel);
        tracing::info!("connecting to RFCOMM {}", socket_addr);
        let stream = bluer::rfcomm::Stream::connect(socket_addr)
            .await
            .map_err(|err| {
                SonyError::Io(std::io::Error::other(format!(
                    "RFCOMM connect failed: {}",
                    err
                )))
            })?;
        Ok(Self::new(stream))
    }
}
