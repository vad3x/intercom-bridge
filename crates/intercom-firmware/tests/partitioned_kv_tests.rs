use intercom_firmware::reset::PartitionedKvBlobStore;
use rs_matter::error::Error;
use rs_matter::persist::{KvBlobStore, BASIC_INFO_KEY, NETWORKS_KEY};

// Mock in-memory KvBlobStore for testing
struct KvStoreMock {
    items: [(u16, Option<[u8; 512]>, usize); 8],
}

impl KvStoreMock {
    fn new() -> Self {
        Self {
            items: [(0, None, 0); 8],
        }
    }
}

impl KvBlobStore for KvStoreMock {
    fn load<'a>(&mut self, key: u16, buf: &'a mut [u8]) -> Result<Option<&'a [u8]>, Error> {
        for (_, data, len) in &self.items {
            if data.is_some() && *len > buf.len() {
                return Err(rs_matter::error::ErrorCode::BufferTooSmall.into());
            }
        }
        for (k, data, len) in &self.items {
            if *k == key {
                if let Some(bytes) = data {
                    buf[..*len].copy_from_slice(&bytes[..*len]);
                    return Ok(Some(&buf[..*len]));
                }
            }
        }
        Ok(None)
    }

    fn store(&mut self, key: u16, data: &[u8], _buf: &mut [u8]) -> Result<(), Error> {
        for (k, stored, len) in &mut self.items {
            if *k == key || stored.is_none() {
                *k = key;
                let mut arr = [0u8; 512];
                arr[..data.len()].copy_from_slice(data);
                *stored = Some(arr);
                *len = data.len();
                return Ok(());
            }
        }
        panic!("Mock store full");
    }

    fn remove(&mut self, key: u16, buf: &mut [u8]) -> Result<(), Error> {
        for (_, data, len) in &self.items {
            if data.is_some() && *len > buf.len() {
                return Err(rs_matter::error::ErrorCode::BufferTooSmall.into());
            }
        }
        for (k, stored, len) in &mut self.items {
            if *k == key {
                *stored = None;
                *len = 0;
            }
        }
        Ok(())
    }
}

#[test]
fn test_partitioned_kv_store_routes_networks_key_to_system_store() {
    // Arrange
    let matter_store = KvStoreMock::new();
    let system_store = KvStoreMock::new();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, system_store);
    let mut buf = [0u8; 128];

    // Act
    partitioned
        .store(NETWORKS_KEY, b"ssid_secret_pass", &mut buf)
        .unwrap();

    // Assert
    let mut read_buf = [0u8; 128];
    assert_eq!(
        partitioned
            .system_store
            .load(NETWORKS_KEY, &mut read_buf)
            .unwrap(),
        Some(b"ssid_secret_pass".as_slice())
    );
    assert_eq!(
        partitioned
            .matter_store
            .load(NETWORKS_KEY, &mut read_buf)
            .unwrap(),
        None
    );
}

#[test]
fn test_partitioned_kv_store_routes_door_duration_key_to_system_store() {
    let matter_store = KvStoreMock::new();
    let system_store = KvStoreMock::new();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, system_store);
    let mut scratch = [0u8; 32];

    const DOOR_DURATION_KEY: u16 = 0x1001;
    let duration_bytes = 7u32.to_le_bytes();
    partitioned
        .store(DOOR_DURATION_KEY, &duration_bytes, &mut scratch)
        .unwrap();

    // Verify written to system store, not matter store
    assert_eq!(
        partitioned
            .matter_store
            .load(DOOR_DURATION_KEY, &mut scratch)
            .unwrap(),
        None
    );
    assert_eq!(
        partitioned
            .system_store
            .load(DOOR_DURATION_KEY, &mut scratch)
            .unwrap(),
        Some(duration_bytes.as_slice())
    );
    assert_eq!(
        partitioned.load(DOOR_DURATION_KEY, &mut scratch).unwrap(),
        Some(duration_bytes.as_slice())
    );
}

#[test]
fn test_partitioned_kv_store_routes_fabric_keys_to_matter_store() {
    // Arrange
    let matter_store = KvStoreMock::new();
    let system_store = KvStoreMock::new();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, system_store);
    let mut buf = [0u8; 128];

    // Act
    partitioned
        .store(BASIC_INFO_KEY, b"basic_info_data", &mut buf)
        .unwrap();
    partitioned
        .store(1, b"fabric_1_credential", &mut buf)
        .unwrap();

    // Assert
    let mut read_buf = [0u8; 128];
    assert_eq!(
        partitioned
            .matter_store
            .load(BASIC_INFO_KEY, &mut read_buf)
            .unwrap(),
        Some(b"basic_info_data".as_slice())
    );
    assert_eq!(
        partitioned.matter_store.load(1, &mut read_buf).unwrap(),
        Some(b"fabric_1_credential".as_slice())
    );
    assert_eq!(
        partitioned
            .system_store
            .load(BASIC_INFO_KEY, &mut read_buf)
            .unwrap(),
        None
    );
    assert_eq!(
        partitioned.system_store.load(1, &mut read_buf).unwrap(),
        None
    );
}

#[test]
fn test_partitioned_kv_store_empty_returns_none() {
    // Arrange
    let matter_store = KvStoreMock::new();
    let system_store = KvStoreMock::new();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, system_store);
    let mut buf1 = [0u8; 128];
    let mut buf2 = [0u8; 128];

    // Act
    let loaded_networks = partitioned.load(NETWORKS_KEY, &mut buf1).unwrap();
    let loaded_basic = partitioned.load(BASIC_INFO_KEY, &mut buf2).unwrap();

    // Assert
    assert_eq!(loaded_networks, None);
    assert_eq!(loaded_basic, None);
}

#[test]
fn test_partitioned_kv_store_load_networks_key_ignores_matter_store() {
    // Arrange
    let mut matter_store = KvStoreMock::new();
    let system_store = KvStoreMock::new();
    let mut setup_buf = [0u8; 128];
    matter_store
        .store(NETWORKS_KEY, b"legacy_matter_creds", &mut setup_buf)
        .unwrap();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, system_store);
    let mut buf = [0u8; 128];

    // Act
    let loaded = partitioned.load(NETWORKS_KEY, &mut buf).unwrap();

    // Assert
    assert_eq!(loaded, None);
}

#[test]
fn test_partitioned_kv_store_remove_networks_key_only_touches_system_store() {
    // Arrange
    let mut matter_store = KvStoreMock::new();
    let mut system_store = KvStoreMock::new();
    let mut setup_buf = [0u8; 128];
    matter_store
        .store(NETWORKS_KEY, b"matter_copy", &mut setup_buf)
        .unwrap();
    system_store
        .store(NETWORKS_KEY, b"system_copy", &mut setup_buf)
        .unwrap();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, system_store);
    let mut buf = [0u8; 128];

    // Act
    partitioned.remove(NETWORKS_KEY, &mut buf).unwrap();

    // Assert
    assert_eq!(
        partitioned
            .system_store
            .load(NETWORKS_KEY, &mut buf)
            .unwrap(),
        None
    );
    assert_eq!(
        partitioned
            .matter_store
            .load(NETWORKS_KEY, &mut buf)
            .unwrap(),
        Some(b"matter_copy".as_slice())
    );
}

#[test]
fn test_partitioned_kv_store_remove_matter_key_only_touches_matter_store() {
    // Arrange
    let matter_store = KvStoreMock::new();
    let system_store = KvStoreMock::new();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, system_store);
    let mut buf = [0u8; 128];
    partitioned
        .store(NETWORKS_KEY, b"system_safe", &mut buf)
        .unwrap();
    partitioned
        .store(BASIC_INFO_KEY, b"info_data", &mut buf)
        .unwrap();

    // Act
    partitioned.remove(BASIC_INFO_KEY, &mut buf).unwrap();

    // Assert
    assert_eq!(partitioned.load(BASIC_INFO_KEY, &mut buf).unwrap(), None);
    assert_eq!(
        partitioned.load(NETWORKS_KEY, &mut buf).unwrap(),
        Some(b"system_safe".as_slice())
    );
}

#[test]
fn test_partitioned_kv_store_update_existing_networks_key() {
    // Arrange
    let matter_store = KvStoreMock::new();
    let system_store = KvStoreMock::new();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, system_store);
    let mut buf = [0u8; 128];
    partitioned
        .store(NETWORKS_KEY, b"old_ssid_pass", &mut buf)
        .unwrap();

    // Act
    partitioned
        .store(NETWORKS_KEY, b"new_ssid_pass", &mut buf)
        .unwrap();

    // Assert
    assert_eq!(
        partitioned.load(NETWORKS_KEY, &mut buf).unwrap(),
        Some(b"new_ssid_pass".as_slice())
    );
}

#[test]
fn test_partitioned_kv_store_buffer_too_small_returns_error() {
    // Arrange
    let matter_store = KvStoreMock::new();
    let mut system_store = KvStoreMock::new();
    let mut setup_buf = [0u8; 128];
    system_store
        .store(NETWORKS_KEY, b"long_credentials_data", &mut setup_buf)
        .unwrap();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, system_store);
    let mut tiny_buf = [0u8; 4];

    // Act
    let result = partitioned.load(NETWORKS_KEY, &mut tiny_buf);

    // Assert
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().code(),
        rs_matter::error::ErrorCode::BufferTooSmall
    );
}

#[test]
fn test_load_door_duration_defaults_when_empty() {
    let mut store = KvStoreMock::new();
    assert_eq!(
        intercom_firmware::reset::load_door_duration(&mut store),
        intercom_core::config::DEFAULT_DOOR_UNLOCK_DURATION_SEC
    );
}

#[test]
fn test_save_and_load_door_duration_persists_valid_value() {
    let mut store = KvStoreMock::new();
    assert!(intercom_firmware::reset::save_door_duration(&mut store, 12).is_ok());
    assert_eq!(intercom_firmware::reset::load_door_duration(&mut store), 12);
}

#[test]
fn test_save_door_duration_clamps_out_of_bounds() {
    let mut store = KvStoreMock::new();

    // Value 0 clamped to MIN (1)
    assert!(intercom_firmware::reset::save_door_duration(&mut store, 0).is_ok());
    assert_eq!(intercom_firmware::reset::load_door_duration(&mut store), 1);

    // Value 100 clamped to MAX (30)
    assert!(intercom_firmware::reset::save_door_duration(&mut store, 100).is_ok());
    assert_eq!(intercom_firmware::reset::load_door_duration(&mut store), 30);
}

#[test]
fn test_load_door_duration_falls_back_on_invalid_or_corrupt_data() {
    let mut store = KvStoreMock::new();
    let mut scratch = [0u8; 32];

    // Too few bytes (< 4)
    store
        .store(
            intercom_firmware::config::DOOR_DURATION_KEY,
            &[1, 2],
            &mut scratch,
        )
        .unwrap();
    assert_eq!(
        intercom_firmware::reset::load_door_duration(&mut store),
        intercom_core::config::DEFAULT_DOOR_UNLOCK_DURATION_SEC
    );

    // Out of bounds (< 1)
    store
        .store(
            intercom_firmware::config::DOOR_DURATION_KEY,
            &0u32.to_le_bytes(),
            &mut scratch,
        )
        .unwrap();
    assert_eq!(
        intercom_firmware::reset::load_door_duration(&mut store),
        intercom_core::config::DEFAULT_DOOR_UNLOCK_DURATION_SEC
    );

    // Out of bounds (> 30)
    store
        .store(
            intercom_firmware::config::DOOR_DURATION_KEY,
            &31u32.to_le_bytes(),
            &mut scratch,
        )
        .unwrap();
    assert_eq!(
        intercom_firmware::reset::load_door_duration(&mut store),
        intercom_core::config::DEFAULT_DOOR_UNLOCK_DURATION_SEC
    );
}

#[test]
fn test_load_door_duration_succeeds_when_larger_network_key_stored() {
    let mut store = KvStoreMock::new();
    let mut scratch = [0u8; 128];
    store
        .store(NETWORKS_KEY, &[0xAA; 40], &mut scratch)
        .unwrap();
    intercom_firmware::reset::save_door_duration(&mut store, 10).unwrap();
    let loaded = intercom_firmware::reset::load_door_duration(&mut store);
    assert_eq!(loaded, 10, "Expected 10 but got {}", loaded);
}

#[test]
fn audio_gains_persist_in_system_and_follow_reset_scopes() {
    // Arrange
    use intercom_core::domain::policy::{RxPgaGain, TxDacGain};
    use intercom_firmware::{
        config::{AUDIO_RX_GAIN_KEY, AUDIO_TX_GAIN_KEY},
        reset::{load_audio_gains, save_audio_gains},
    };
    let mut store = PartitionedKvBlobStore::new(KvStoreMock::new(), KvStoreMock::new());
    let gains = (RxPgaGain::clamped(52), TxDacGain::clamped(-12));
    let mut scratch = [0; 64];

    // Act
    save_audio_gains(&mut store, gains.0, gains.1).unwrap();
    store.matter_store = KvStoreMock::new();
    let ecosystem_gains = load_audio_gains(&mut store);

    // Assert
    assert_eq!(ecosystem_gains, gains);
    assert_eq!(
        store
            .system_store
            .load(AUDIO_RX_GAIN_KEY, &mut scratch)
            .unwrap(),
        Some([52].as_slice())
    );
    assert_eq!(
        store
            .system_store
            .load(AUDIO_TX_GAIN_KEY, &mut scratch)
            .unwrap(),
        Some([(-12i8) as u8].as_slice())
    );
    assert_eq!(
        store
            .matter_store
            .load(AUDIO_RX_GAIN_KEY, &mut scratch)
            .unwrap(),
        None
    );
    assert_eq!(
        store
            .matter_store
            .load(AUDIO_TX_GAIN_KEY, &mut scratch)
            .unwrap(),
        None
    );

    // Act: factory reset erases both partitions.
    store.system_store = KvStoreMock::new();
    store.matter_store = KvStoreMock::new();
    let factory_gains = load_audio_gains(&mut store);

    // Assert
    assert_eq!(factory_gains, (RxPgaGain::default(), TxDacGain::default()));
}

#[test]
fn audio_gain_loading_defaults_missing_empty_invalid_and_read_errors() {
    // Arrange
    use intercom_core::domain::policy::{RxPgaGain, TxDacGain};
    use intercom_firmware::{
        config::{AUDIO_RX_GAIN_KEY, AUDIO_TX_GAIN_KEY},
        reset::load_audio_gains,
    };
    let mut store = KvStoreMock::new();
    let mut scratch = [0; 128];

    // Act
    let missing = load_audio_gains(&mut store);

    // Assert
    assert_eq!(missing, (RxPgaGain::default(), TxDacGain::default()));
    for (rx, tx) in [
        (&[][..], &[][..]),
        (&[96][..], &[49][..]),
        (&[255][..], &[128][..]),
    ] {
        // Arrange
        store.store(AUDIO_RX_GAIN_KEY, rx, &mut scratch).unwrap();
        store.store(AUDIO_TX_GAIN_KEY, tx, &mut scratch).unwrap();

        // Act
        let loaded = load_audio_gains(&mut store);

        // Assert
        assert_eq!(loaded, missing);
    }
    // Arrange: the existing mock returns BufferTooSmall when any blob exceeds scratch.
    store.store(NETWORKS_KEY, &[0; 100], &mut scratch).unwrap();

    // Act
    let read_error = load_audio_gains(&mut store);

    // Assert
    assert_eq!(read_error, missing);
}

#[test]
fn gain_value_objects_validate_clamp_and_default() {
    // Arrange
    use intercom_core::domain::policy::{RxPgaGain, TxDacGain};

    // Act
    let rx_default = RxPgaGain::default();
    let tx_default = TxDacGain::default();

    // Assert
    assert_eq!(rx_default.half_db(), 40);
    assert_eq!(tx_default.half_db(), 0);
    for value in 0..=95 {
        assert_eq!(RxPgaGain::try_new(value).unwrap().half_db(), value);
    }
    assert_eq!(RxPgaGain::try_new(96), None);
    assert_eq!(RxPgaGain::try_new(255), None);
    assert_eq!(RxPgaGain::clamped(255).half_db(), 95);
    for value in -127..=48 {
        assert_eq!(TxDacGain::try_new(value).unwrap().half_db(), value);
    }
    assert_eq!(TxDacGain::try_new(-128), None);
    assert_eq!(TxDacGain::try_new(49), None);
    assert_eq!(TxDacGain::clamped(-128).half_db(), -127);
    assert_eq!(TxDacGain::clamped(127).half_db(), 48);
}

#[test]
fn persistence_reporter_derives_tone_from_session_owner_and_phase() {
    // Arrange
    use intercom_core::{
        domain::{
            diagnostics,
            intercom::{CallState, SessionOwner},
        },
        ports::IntercomReporter,
    };
    let reporter = intercom_firmware::reset::FirmwarePersistenceReporter;

    // Act
    for (state, owner, active) in [
        (CallState::Talking, Some(SessionOwner::BenchTone), true),
        (CallState::Settling, Some(SessionOwner::BenchTone), true),
        (CallState::Listening, Some(SessionOwner::BenchTone), false),
        (CallState::Talking, Some(SessionOwner::MatterCall), false),
        (CallState::Idle, None, false),
    ] {
        reporter.report_session_state(state, owner);

        // Assert
        assert_eq!(diagnostics::is_tone_active(), active);
    }
}

#[test]
fn audio_telemetry_reports_levels_gains_and_defaults() {
    // Arrange
    use intercom_core::domain::{
        diagnostics,
        policy::{RxPgaGain, TxDacGain},
    };
    use intercom_core::ports::IntercomReporter;
    let reporter = intercom_firmware::reset::FirmwarePersistenceReporter;
    let default = diagnostics::IntercomState::default();

    // Act
    reporter.report_audio_gain(RxPgaGain::clamped(52), TxDacGain::clamped(-12));
    diagnostics::update_rx_levels_dbfs(-32, -9);
    let snapshot = diagnostics::current_intercom_state();
    reporter.report_audio_gain(RxPgaGain::default(), TxDacGain::default());
    diagnostics::update_rx_levels_dbfs(-96, -96);

    // Assert
    assert_eq!(
        (snapshot.rx_gain_half_db, snapshot.tx_gain_half_db),
        (52, -12)
    );
    assert_eq!((snapshot.rx_rms_dbfs, snapshot.rx_peak_dbfs), (-32, -9));
    assert_eq!((default.rx_gain_half_db, default.tx_gain_half_db), (40, 0));
    assert_eq!((default.rx_rms_dbfs, default.rx_peak_dbfs), (-96, -96));
    assert!(!default.tone_active);
}
