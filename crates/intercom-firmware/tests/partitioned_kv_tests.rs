use intercom_firmware::reset::PartitionedKvBlobStore;
use rs_matter::error::Error;
use rs_matter::persist::{KvBlobStore, BASIC_INFO_KEY, NETWORKS_KEY};

// Mock in-memory KvBlobStore for testing
struct MockKvStore {
    items: [(u16, Option<[u8; 64]>, usize); 8],
}

impl MockKvStore {
    fn new() -> Self {
        Self {
            items: [(0, None, 0); 8],
        }
    }
}

impl KvBlobStore for MockKvStore {
    fn load<'a>(&mut self, key: u16, buf: &'a mut [u8]) -> Result<Option<&'a [u8]>, Error> {
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
                let mut arr = [0u8; 64];
                arr[..data.len()].copy_from_slice(data);
                *stored = Some(arr);
                *len = data.len();
                return Ok(());
            }
        }
        panic!("Mock store full");
    }

    fn remove(&mut self, key: u16, _buf: &mut [u8]) -> Result<(), Error> {
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
fn test_partitioned_kv_store_routes_networks_key_to_wifi_store() {
    // Arrange
    let matter_store = MockKvStore::new();
    let wifi_store = MockKvStore::new();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, wifi_store);
    let mut buf = [0u8; 128];

    // Act
    partitioned
        .store(NETWORKS_KEY, b"ssid_secret_pass", &mut buf)
        .unwrap();

    // Assert
    let mut read_buf = [0u8; 128];
    assert_eq!(
        partitioned
            .wifi_store
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
fn test_partitioned_kv_store_routes_fabric_keys_to_matter_store() {
    // Arrange
    let matter_store = MockKvStore::new();
    let wifi_store = MockKvStore::new();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, wifi_store);
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
            .wifi_store
            .load(BASIC_INFO_KEY, &mut read_buf)
            .unwrap(),
        None
    );
    assert_eq!(partitioned.wifi_store.load(1, &mut read_buf).unwrap(), None);
}

#[test]
fn test_partitioned_kv_store_empty_returns_none() {
    // Arrange
    let matter_store = MockKvStore::new();
    let wifi_store = MockKvStore::new();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, wifi_store);
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
fn test_partitioned_kv_store_auto_migrates_legacy_networks_key() {
    // Arrange
    let mut matter_store = MockKvStore::new();
    let wifi_store = MockKvStore::new();
    let mut setup_buf = [0u8; 128];
    matter_store
        .store(NETWORKS_KEY, b"migrated_wifi_creds", &mut setup_buf)
        .unwrap();

    let mut partitioned = PartitionedKvBlobStore::new(matter_store, wifi_store);
    let mut read_buf = [0u8; 128];

    // Act
    let loaded = partitioned.load(NETWORKS_KEY, &mut read_buf).unwrap();

    // Assert
    assert_eq!(loaded, Some(b"migrated_wifi_creds".as_slice()));
    let mut check_buf = [0u8; 128];
    assert_eq!(
        partitioned
            .wifi_store
            .load(NETWORKS_KEY, &mut check_buf)
            .unwrap(),
        Some(b"migrated_wifi_creds".as_slice())
    );
    assert_eq!(
        partitioned
            .matter_store
            .load(NETWORKS_KEY, &mut check_buf)
            .unwrap(),
        None
    );
}

#[test]
fn test_partitioned_kv_store_migration_only_happens_once() {
    // Arrange
    let mut matter_store = MockKvStore::new();
    let wifi_store = MockKvStore::new();
    let mut setup_buf = [0u8; 128];
    matter_store
        .store(NETWORKS_KEY, b"first_creds", &mut setup_buf)
        .unwrap();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, wifi_store);
    let mut buf1 = [0u8; 128];
    let mut buf2 = [0u8; 128];
    let mut store_buf = [0u8; 128];

    // Act
    let first = partitioned.load(NETWORKS_KEY, &mut buf1).unwrap();
    partitioned
        .store(NETWORKS_KEY, b"updated_creds", &mut store_buf)
        .unwrap();
    let second = partitioned.load(NETWORKS_KEY, &mut buf2).unwrap();

    // Assert
    assert_eq!(first, Some(b"first_creds".as_slice()));
    assert_eq!(second, Some(b"updated_creds".as_slice()));
}

#[test]
fn test_partitioned_kv_store_remove_networks_key_removes_from_both() {
    // Arrange
    let mut matter_store = MockKvStore::new();
    let mut wifi_store = MockKvStore::new();
    let mut setup_buf = [0u8; 128];
    matter_store
        .store(NETWORKS_KEY, b"matter_copy", &mut setup_buf)
        .unwrap();
    wifi_store
        .store(NETWORKS_KEY, b"wifi_copy", &mut setup_buf)
        .unwrap();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, wifi_store);
    let mut buf = [0u8; 128];

    // Act
    partitioned.remove(NETWORKS_KEY, &mut buf).unwrap();

    // Assert
    assert_eq!(
        partitioned.wifi_store.load(NETWORKS_KEY, &mut buf).unwrap(),
        None
    );
    assert_eq!(
        partitioned
            .matter_store
            .load(NETWORKS_KEY, &mut buf)
            .unwrap(),
        None
    );
}

#[test]
fn test_partitioned_kv_store_remove_matter_key_only_touches_matter_store() {
    // Arrange
    let matter_store = MockKvStore::new();
    let wifi_store = MockKvStore::new();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, wifi_store);
    let mut buf = [0u8; 128];
    partitioned
        .store(NETWORKS_KEY, b"wifi_safe", &mut buf)
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
        Some(b"wifi_safe".as_slice())
    );
}

#[test]
fn test_partitioned_kv_store_update_existing_networks_key() {
    // Arrange
    let matter_store = MockKvStore::new();
    let wifi_store = MockKvStore::new();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, wifi_store);
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

struct FailingStore;

impl KvBlobStore for FailingStore {
    fn load<'a>(&mut self, _key: u16, _buf: &'a mut [u8]) -> Result<Option<&'a [u8]>, Error> {
        Ok(None)
    }

    fn store(&mut self, _key: u16, _data: &[u8], _buf: &mut [u8]) -> Result<(), Error> {
        Err(rs_matter::error::ErrorCode::ResourceExhausted.into())
    }

    fn remove(&mut self, _key: u16, _buf: &mut [u8]) -> Result<(), Error> {
        Ok(())
    }
}

#[test]
fn test_partitioned_kv_store_buffer_too_small_returns_error() {
    // Arrange
    let matter_store = MockKvStore::new();
    let mut wifi_store = MockKvStore::new();
    let mut setup_buf = [0u8; 128];
    wifi_store
        .store(NETWORKS_KEY, b"long_credentials_data", &mut setup_buf)
        .unwrap();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, wifi_store);
    let mut tiny_buf = [0u8; 4];

    // Act
    let result = partitioned.load(NETWORKS_KEY, &mut tiny_buf);

    // Assert
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), rs_matter::error::ErrorCode::BufferTooSmall);
}

#[test]
fn test_partitioned_kv_store_legacy_buffer_too_small_returns_error() {
    // Arrange
    let mut matter_store = MockKvStore::new();
    let wifi_store = MockKvStore::new();
    let mut setup_buf = [0u8; 128];
    matter_store
        .store(NETWORKS_KEY, b"long_legacy_credentials", &mut setup_buf)
        .unwrap();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, wifi_store);
    let mut tiny_buf = [0u8; 4];

    // Act
    let result = partitioned.load(NETWORKS_KEY, &mut tiny_buf);

    // Assert
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), rs_matter::error::ErrorCode::BufferTooSmall);
}

#[test]
fn test_partitioned_kv_store_migration_populates_buf_even_if_wifi_store_fails() {
    // Arrange
    let mut matter_store = MockKvStore::new();
    let wifi_store = FailingStore;
    let mut setup_buf = [0u8; 128];
    matter_store
        .store(NETWORKS_KEY, b"legacy_wifi_creds", &mut setup_buf)
        .unwrap();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, wifi_store);
    let mut read_buf = [0u8; 128];

    // Act
    let result = partitioned.load(NETWORKS_KEY, &mut read_buf);

    // Assert
    assert_eq!(result.unwrap(), Some(b"legacy_wifi_creds".as_slice()));
    assert_eq!(&read_buf[..17], b"legacy_wifi_creds");
}
