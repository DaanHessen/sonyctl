use sony_api::protocol::{decode_frame, DataType, Frame, END, ESCAPE, START};

#[test]
fn encodes_a_minimal_frame() {
    // Payload 0x01, dataType DataMdr (0x0c), seq 0.
    // checksum = 0x0c + 0x00 + 0x00+0x00+0x00+0x01 + 0x01 = 0x0e
    let frame = Frame::new(DataType::DataMdr, 0, vec![0x01]);
    assert_eq!(
        frame.encode(),
        vec![START, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x0e, END]
    );
}

#[test]
fn encodes_an_ack() {
    // dataType 0x01, seq 1, empty payload. checksum = 0x01 + 0x01 = 0x02
    let frame = Frame::ack(1);
    assert_eq!(
        frame.encode(),
        vec![START, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x02, END]
    );
    assert!(frame.is_ack());
}

#[test]
fn escapes_reserved_bytes_in_the_payload() {
    let frame = Frame::new(DataType::DataMdr, 0, vec![0x3e, 0x3c, 0x3d]);
    let encoded = frame.encode();
    // Reserved bytes become ESCAPE + (byte & 0xEF).
    let payload_section = &encoded[7..13];
    assert_eq!(payload_section, &[ESCAPE, 0x2e, ESCAPE, 0x2c, ESCAPE, 0x2d]);
    // START and END markers are never escaped.
    assert_eq!(encoded[0], START);
    assert_eq!(*encoded.last().unwrap(), END);
}

#[test]
fn escapes_a_reserved_checksum() {
    // A payload whose checksum lands on 0x3c must be escaped too.
    // dataType 0x0c + seq 0 + length bytes 0,0,0,1 + payload P == 0x3c
    // => P = 0x3c - 0x0c - 0x01 = 0x2f
    let frame = Frame::new(DataType::DataMdr, 0, vec![0x2f]);
    let encoded = frame.encode();
    assert_eq!(&encoded[encoded.len() - 3..], &[ESCAPE, 0x2c, END]);
}

#[test]
fn round_trips_every_frame() {
    for payload in [
        vec![],
        vec![0x00],
        vec![0x3e, 0x3c, 0x3d],
        (0u8..=255).collect::<Vec<u8>>(),
    ] {
        let original = Frame::new(DataType::DataMdr, 1, payload.clone());
        let mut buf = original.encode();
        let decoded = decode_frame(&mut buf).unwrap().expect("a complete frame");
        assert_eq!(decoded.payload, payload);
        assert_eq!(decoded.seq, 1);
        assert_eq!(decoded.data_type.to_byte(), 0x0c);
        assert!(buf.is_empty(), "the decoder must consume the frame");
    }
}

#[test]
fn returns_none_for_a_partial_frame() {
    let encoded = Frame::new(DataType::DataMdr, 0, vec![0x01, 0x02]).encode();
    let mut buf = encoded[..encoded.len() - 1].to_vec();
    let before = buf.clone();
    assert!(decode_frame(&mut buf).unwrap().is_none());
    assert_eq!(buf, before, "a partial frame must be left in the buffer");
}

#[test]
fn decodes_concatenated_frames_one_at_a_time() {
    let mut buf = Frame::new(DataType::DataMdr, 0, vec![0xaa]).encode();
    buf.extend(Frame::ack(1).encode());

    let first = decode_frame(&mut buf).unwrap().unwrap();
    assert_eq!(first.payload, vec![0xaa]);

    let second = decode_frame(&mut buf).unwrap().unwrap();
    assert!(second.is_ack());
    assert_eq!(second.seq, 1);

    assert!(buf.is_empty());
    assert!(decode_frame(&mut buf).unwrap().is_none());
}

#[test]
fn skips_leading_junk_before_a_start_marker() {
    let mut buf = vec![0x00, 0xff];
    buf.extend(Frame::new(DataType::DataMdr, 0, vec![0x07]).encode());
    let frame = decode_frame(&mut buf).unwrap().unwrap();
    assert_eq!(frame.payload, vec![0x07]);
}

#[test]
fn rejects_a_bad_checksum() {
    let mut buf = Frame::new(DataType::DataMdr, 0, vec![0x01]).encode();
    let checksum_index = buf.len() - 2;
    buf[checksum_index] = buf[checksum_index].wrapping_add(1);
    assert!(decode_frame(&mut buf).is_err());
}
