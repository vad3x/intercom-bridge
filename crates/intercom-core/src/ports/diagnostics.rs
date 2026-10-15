//! Hardware and system telemetry reporting port.

/// Hardware telemetry provider port.
///
/// Implementors query the underlying platform runtime, network stack,
/// bootloader, and build metadata for diagnostics snapshot aggregation.
pub trait HardwareInfoDriver {
    /// Return system uptime in seconds.
    fn uptime_secs(&self) -> u64;

    /// Return available free heap in bytes.
    fn free_heap_bytes(&self) -> usize;

    /// Return total heap size in bytes.
    fn total_heap_bytes(&self) -> usize;

    /// Return Wi-Fi RSSI in dBm if connected, or None.
    fn wifi_rssi(&self) -> Option<i8>;

    /// Return IPv4 address if assigned, or None.
    fn ip_address(&self) -> Option<[u8; 4]>;

    /// Return station MAC address as a 6-byte array.
    fn mac_address(&self) -> [u8; 6];

    /// Return the active boot partition slot identifier (e.g. "ota_0" or "ota_1").
    fn active_slot(&self) -> &'static str;

    /// Return whether the active slot is marked valid in otadata.
    fn is_slot_valid(&self) -> bool;

    /// Return the semantic firmware version string.
    fn firmware_version(&self) -> &'static str;

    /// Return the Git commit hash of the build.
    fn git_commit(&self) -> &'static str;

    /// Return the build timestamp string.
    fn build_date(&self) -> &'static str;
}
