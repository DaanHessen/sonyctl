//! Tests that need the headphones connected. Run with:
//!   cargo test --test live -- --ignored

const DEVICE: &str = "14:3F:A6:DB:86:A9";

#[tokio::test]
#[ignore = "requires the headphones to be connected"]
async fn resolves_the_sony_channel() {
    let channel = sony_api::bluetooth::detect_rfcomm_channel(DEVICE)
        .await
        .unwrap();
    assert_eq!(channel, 9);
}
