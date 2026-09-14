use crate::error::SonyError;

pub const START: u8 = 0x3e;
pub const END: u8 = 0x3c;
pub const ESCAPE: u8 = 0x3d;
/// Reserved bytes are escaped by clearing bit 4; unescaping sets it again.
const ESCAPE_MASK: u8 = 0xEF;
const UNESCAPE_MASK: u8 = 0x10;

const HEADER_LEN: usize = 6; // dataType + seq + u32 length

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    Ack,
    DataMdr,
    DataMdrNo2,
    Other(u8),
}

impl DataType {
    pub fn to_byte(self) -> u8 {
        match self {
            DataType::Ack => 0x01,
            DataType::DataMdr => 0x0c,
            DataType::DataMdrNo2 => 0x0e,
            DataType::Other(byte) => byte,
        }
    }

    pub fn from_byte(byte: u8) -> DataType {
        match byte {
            0x01 => DataType::Ack,
            0x0c => DataType::DataMdr,
            0x0e => DataType::DataMdrNo2,
            other => DataType::Other(other),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub data_type: DataType,
    pub seq: u8,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn new(data_type: DataType, seq: u8, payload: Vec<u8>) -> Frame {
        Frame {
            data_type,
            seq,
            payload,
        }
    }

    pub fn ack(seq: u8) -> Frame {
        Frame::new(DataType::Ack, seq, Vec::new())
    }

    pub fn is_ack(&self) -> bool {
        self.data_type == DataType::Ack
    }

    /// The frame body as it is checksummed: header bytes followed by the
    /// payload, before any escaping is applied.
    fn body(&self) -> Vec<u8> {
        let mut body = Vec::with_capacity(HEADER_LEN + self.payload.len());
        body.push(self.data_type.to_byte());
        body.push(self.seq);
        body.extend_from_slice(&(self.payload.len() as u32).to_be_bytes());
        body.extend_from_slice(&self.payload);
        body
    }

    pub fn encode(&self) -> Vec<u8> {
        let body = self.body();
        let checksum = checksum(&body);

        let mut out = Vec::with_capacity(body.len() + 4);
        out.push(START);
        for byte in body.iter().chain(std::iter::once(&checksum)) {
            push_escaped(&mut out, *byte);
        }
        out.push(END);
        out
    }
}

fn checksum(body: &[u8]) -> u8 {
    body.iter().fold(0u8, |acc, byte| acc.wrapping_add(*byte))
}

fn push_escaped(out: &mut Vec<u8>, byte: u8) {
    if matches!(byte, START | END | ESCAPE) {
        out.push(ESCAPE);
        out.push(byte & ESCAPE_MASK);
    } else {
        out.push(byte);
    }
}

/// Consume one complete frame from the front of `buf`.
///
/// Returns `Ok(None)` when `buf` does not yet hold a whole frame, leaving it
/// untouched so the caller can read more bytes and try again. Bytes before
/// the first START marker are discarded as line noise.
pub fn decode_frame(buf: &mut Vec<u8>) -> Result<Option<Frame>, SonyError> {
    let Some(start) = buf.iter().position(|byte| *byte == START) else {
        return Ok(None);
    };
    let Some(offset) = buf[start + 1..].iter().position(|byte| *byte == END) else {
        return Ok(None);
    };
    let end = start + 1 + offset;

    let unescaped = unescape(&buf[start + 1..end])?;
    buf.drain(..=end);

    if unescaped.len() < HEADER_LEN + 1 {
        return Err(SonyError::InvalidFrame);
    }

    let (body, checksum_byte) = unescaped.split_at(unescaped.len() - 1);
    if checksum(body) != checksum_byte[0] {
        return Err(SonyError::ChecksumMismatch);
    }

    let declared_len = u32::from_be_bytes([body[2], body[3], body[4], body[5]]) as usize;
    let payload = &body[HEADER_LEN..];
    if payload.len() != declared_len {
        return Err(SonyError::InvalidFrame);
    }

    Ok(Some(Frame {
        data_type: DataType::from_byte(body[0]),
        seq: body[1],
        payload: payload.to_vec(),
    }))
}

fn unescape(bytes: &[u8]) -> Result<Vec<u8>, SonyError> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut iter = bytes.iter();
    while let Some(byte) = iter.next() {
        if *byte == ESCAPE {
            let next = iter.next().ok_or(SonyError::InvalidFrame)?;
            out.push(next | UNESCAPE_MASK);
        } else {
            out.push(*byte);
        }
    }
    Ok(out)
}
