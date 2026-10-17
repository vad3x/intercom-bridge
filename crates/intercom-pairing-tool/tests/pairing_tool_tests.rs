use rs_matter::pairing::qr::{no_optional_data, CommFlowType, QrPayload};
use rs_matter::pairing::DiscoveryCapabilities;
use rs_matter::BasicCommData;

#[test]
fn test_qr_payload_round_trip() {
    // Arrange
    let mac = [0x98, 0xa3, 0x16, 0xa6, 0x78, 0x44];
    let passcode = intercom_core::domain::commissioning::passcode_from_mac(&mac);
    let discriminator = intercom_core::domain::commissioning::discriminator_from_mac(&mac);

    let comm_data = BasicCommData {
        password: passcode.to_le_bytes().into(),
        discriminator,
    };

    let dev_det = rs_matter::dm::clusters::basic_info::BasicInfoConfig {
        vid: 0xfff1,
        pid: 0x8001,
        ..rs_matter::dm::clusters::basic_info::BasicInfoConfig::new()
    };

    let payload = QrPayload::new_from_basic_info(
        DiscoveryCapabilities::BLE,
        CommFlowType::Standard,
        comm_data,
        &dev_det,
        no_optional_data,
    );

    let mut buf = [0u8; 128];

    // Act
    let (qr_text, _) = payload.as_str(&mut buf).unwrap();

    // Assert
    assert!(qr_text.starts_with("MT:"));
    let mut parse_buf = [0u8; 128];
    let parsed = QrPayload::parse(qr_text, &mut parse_buf).unwrap();
    assert_eq!(parsed.passcode(), passcode);
    assert_eq!(parsed.discriminator(), discriminator);
    assert_eq!(parsed.vid(), 0xfff1);
    assert_eq!(parsed.pid(), 0x8001);
}

#[test]
fn test_manual_code_verhoeff_validity() {
    // Arrange
    let mac = [0x98, 0xa3, 0x16, 0xa6, 0x78, 0x44];
    let passcode = intercom_core::domain::commissioning::passcode_from_mac(&mac);
    let discriminator = intercom_core::domain::commissioning::discriminator_from_mac(&mac);

    let comm_data = BasicCommData {
        password: passcode.to_le_bytes().into(),
        discriminator,
    };

    // Act
    let short_code = comm_data.compute_pairing_code();
    let long_code = comm_data.compute_pairing_code_long(0xfff1, 0x8001);

    // Assert
    assert_eq!(short_code.len(), 11);
    assert_eq!(long_code.len(), 21);
    assert!(QrPayload::parse_pairing_code(&short_code).is_ok());
    assert!(QrPayload::parse_pairing_code(&long_code).is_ok());
}
