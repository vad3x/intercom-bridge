use intercom_firmware::reset::PartitionedKvBlobStore;
use rs_matter::error::Error;
use rs_matter::persist::{KvBlobStore, BASIC_INFO_KEY, NETWORKS_KEY};

// Mock in-memory KvBlobStore for testing
struct MockKvStore {
    items: [(u16, Option<[u8; 512]>, usize); 8],
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
fn test_partitioned_kv_store_load_networks_key_ignores_matter_store() {
    // Arrange
    let mut matter_store = MockKvStore::new();
    let wifi_store = MockKvStore::new();
    let mut setup_buf = [0u8; 128];
    matter_store
        .store(NETWORKS_KEY, b"legacy_matter_creds", &mut setup_buf)
        .unwrap();
    let mut partitioned = PartitionedKvBlobStore::new(matter_store, wifi_store);
    let mut buf = [0u8; 128];

    // Act
    let loaded = partitioned.load(NETWORKS_KEY, &mut buf).unwrap();

    // Assert
    assert_eq!(loaded, None);
}

#[test]
fn test_partitioned_kv_store_remove_networks_key_only_touches_wifi_store() {
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
        Some(b"matter_copy".as_slice())
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
