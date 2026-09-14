use std::time::Duration;

use sony_api::connection::SonyConnection;
use sony_api::protocol::{decode_frame, DataType, Frame};
use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt, DuplexStream};

/// Read exactly one frame from the far end of the duplex.
async fn read_frame(peer: &mut DuplexStream) -> Frame {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 256];
    loop {
        let n = peer.read(&mut chunk).await.unwrap();
        buf.extend_from_slice(&chunk[..n]);
        if let Some(frame) = decode_frame(&mut buf).unwrap() {
            return frame;
        }
    }
}

#[tokio::test]
async fn send_waits_for_an_ack() {
    let (near, mut peer) = duplex(4096);
    let mut conn = SonyConnection::new(near);

    let sender = tokio::spawn(async move {
        conn.send(vec![0x22, 0x00]).await.unwrap();
        conn
    });

    let sent = read_frame(&mut peer).await;
    assert_eq!(sent.payload, vec![0x22, 0x00]);
    assert_eq!(sent.data_type, DataType::DataMdr);
    assert_eq!(sent.seq, 0);

    // The headset acknowledges with the flipped sequence number.
    peer.write_all(&Frame::ack(1).encode()).await.unwrap();
    sender.await.unwrap();
}

#[tokio::test]
async fn send_times_out_without_an_ack() {
    let (near, _peer) = duplex(4096);
    let mut conn = SonyConnection::new(near);
    conn.set_timeout(Duration::from_millis(50));
    assert!(conn.send(vec![0x22, 0x00]).await.is_err());
}

#[tokio::test]
async fn sequence_number_alternates_between_sends() {
    let (near, mut peer) = duplex(4096);
    let mut conn = SonyConnection::new(near);

    let sender = tokio::spawn(async move {
        conn.send(vec![0x01]).await.unwrap();
        conn.send(vec![0x02]).await.unwrap();
    });

    let first = read_frame(&mut peer).await;
    assert_eq!(first.seq, 0);
    peer.write_all(&Frame::ack(1).encode()).await.unwrap();

    let second = read_frame(&mut peer).await;
    assert_eq!(second.seq, 1);
    peer.write_all(&Frame::ack(0).encode()).await.unwrap();

    sender.await.unwrap();
}

#[tokio::test]
async fn recv_acknowledges_the_frame_it_returns() {
    let (near, mut peer) = duplex(4096);
    let mut conn = SonyConnection::new(near);

    peer.write_all(&Frame::new(DataType::DataMdr, 1, vec![0x23, 0x64]).encode())
        .await
        .unwrap();

    let frame = conn.recv().await.unwrap();
    assert_eq!(frame.payload, vec![0x23, 0x64]);

    let ack = read_frame(&mut peer).await;
    assert!(ack.is_ack());
    assert_eq!(ack.seq, 0, "the ACK flips the received sequence number");
}

#[tokio::test]
async fn recv_skips_stray_acks() {
    let (near, mut peer) = duplex(4096);
    let mut conn = SonyConnection::new(near);

    peer.write_all(&Frame::ack(0).encode()).await.unwrap();
    peer.write_all(&Frame::new(DataType::DataMdr, 0, vec![0xaa]).encode())
        .await
        .unwrap();

    let frame = conn.recv().await.unwrap();
    assert_eq!(frame.payload, vec![0xaa]);
}

#[tokio::test]
async fn eof_marks_the_connection_broken() {
    let (near, peer) = duplex(4096);
    let mut conn = SonyConnection::new(near);
    let broken = conn.broken_flag();
    drop(peer);

    assert!(conn.recv().await.is_err());
    assert!(broken.load(std::sync::atomic::Ordering::SeqCst));
}
