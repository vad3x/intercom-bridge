//! Compatibility re-exports and lock-free intercom telemetry snapshots.

use crate::domain::intercom::{BusLineState, CallState};
use crate::domain::policy::{RxPgaGain, TxDacGain, DEFAULT_RX_PGA_HALF_DB, DEFAULT_TX_DAC_HALF_DB};
use core::sync::atomic::{AtomicBool, AtomicI8, AtomicU32, AtomicU8, Ordering};
pub use edf_core::domain::diagnostics::{SystemDiagnostics, WifiSsid};

pub const DEFAULT_SILENCE_DBFS: i8 = -96;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Intercom-specific telemetry domain payload.
pub struct IntercomState {
    pub door_locked: bool,
    pub door_unlock_duration_sec: u32,
    pub bus_state: BusLineState,
    pub call_state: CallState,
    pub rx_rms_dbfs: i8,
    pub rx_peak_dbfs: i8,
    pub rx_gain_half_db: u8,
    pub tx_gain_half_db: i8,
    pub tone_active: bool,
}

pub fn update_rx_levels_dbfs(rms_dbfs: i8, peak_dbfs: i8) {
    RX_RMS_DBFS.store(rms_dbfs, Ordering::Relaxed);
    RX_PEAK_DBFS.store(peak_dbfs, Ordering::Relaxed);
}

pub fn update_audio_gains(rx: RxPgaGain, tx: TxDacGain) {
    RX_GAIN_HALF_DB.store(rx.half_db(), Ordering::Relaxed);
    TX_GAIN_HALF_DB.store(tx.half_db(), Ordering::Relaxed);
}

pub fn update_tone_active(active: bool) {
    TONE_ACTIVE.store(active, Ordering::Relaxed);
}

pub fn is_tone_active() -> bool {
    TONE_ACTIVE.load(Ordering::Relaxed)
}

/// Update the active door lock latch state reflected in telemetry snapshots.
pub fn update_door_locked(locked: bool) {
    DOOR_LOCKED.store(locked, Ordering::Relaxed);
}

/// Update the active door unlock duration reflected in telemetry snapshots.
pub fn update_door_unlock_duration(duration_sec: u32) {
    DOOR_UNLOCK_DURATION.store(duration_sec, Ordering::Relaxed);
}

/// Update the active 1-hot analog bus relay state reflected in telemetry snapshots.
pub fn update_bus_state(state: BusLineState) {
    BUS_STATE.store(state as u8, Ordering::Relaxed);
}

/// Update the active call/session phase reflected in telemetry snapshots.
pub fn update_call_state(state: CallState) {
    CALL_STATE.store(state as u8, Ordering::Relaxed);
}

/// Retrieve the active intercom domain state for telemetry snapshots.
pub fn current_intercom_state() -> IntercomState {
    IntercomState {
        door_locked: DOOR_LOCKED.load(Ordering::Relaxed),
        door_unlock_duration_sec: DOOR_UNLOCK_DURATION.load(Ordering::Relaxed),
        bus_state: BusLineState::from_u8(BUS_STATE.load(Ordering::Relaxed)),
        call_state: CallState::from_u8(CALL_STATE.load(Ordering::Relaxed)),
        rx_rms_dbfs: RX_RMS_DBFS.load(Ordering::Relaxed),
        rx_peak_dbfs: RX_PEAK_DBFS.load(Ordering::Relaxed),
        rx_gain_half_db: RX_GAIN_HALF_DB.load(Ordering::Relaxed),
        tx_gain_half_db: TX_GAIN_HALF_DB.load(Ordering::Relaxed),
        tone_active: TONE_ACTIVE.load(Ordering::Relaxed),
    }
}

impl Default for IntercomState {
    fn default() -> Self {
        Self {
            door_locked: true,
            door_unlock_duration_sec: crate::config::DEFAULT_DOOR_UNLOCK_DURATION_SEC,
            bus_state: BusLineState::Idle,
            call_state: CallState::Idle,
            rx_rms_dbfs: DEFAULT_SILENCE_DBFS,
            rx_peak_dbfs: DEFAULT_SILENCE_DBFS,
            rx_gain_half_db: DEFAULT_RX_PGA_HALF_DB,
            tx_gain_half_db: DEFAULT_TX_DAC_HALF_DB,
            tone_active: false,
        }
    }
}

static DOOR_LOCKED: AtomicBool = AtomicBool::new(true);
static DOOR_UNLOCK_DURATION: AtomicU32 =
    AtomicU32::new(crate::config::DEFAULT_DOOR_UNLOCK_DURATION_SEC);
static BUS_STATE: AtomicU8 = AtomicU8::new(BusLineState::Idle as u8);
static CALL_STATE: AtomicU8 = AtomicU8::new(CallState::Idle as u8);
static RX_RMS_DBFS: AtomicI8 = AtomicI8::new(DEFAULT_SILENCE_DBFS);
static RX_PEAK_DBFS: AtomicI8 = AtomicI8::new(DEFAULT_SILENCE_DBFS);
static RX_GAIN_HALF_DB: AtomicU8 = AtomicU8::new(DEFAULT_RX_PGA_HALF_DB);
static TX_GAIN_HALF_DB: AtomicI8 = AtomicI8::new(DEFAULT_TX_DAC_HALF_DB);
static TONE_ACTIVE: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
mod tests {
    use super::{IntercomState, SystemDiagnostics};
    use crate::events::SystemState;

    #[test]
    fn diagnostics_contract_keeps_intercom_as_generic_domain_payload() {
        // Arrange
        let intercom = IntercomState {
            door_locked: true,
            door_unlock_duration_sec: 5,
            ..IntercomState::default()
        };

        // Act
        let diagnostics = SystemDiagnostics::<IntercomState> {
            device_type: "intercom_bridge",
            state: SystemState::Operational,
            domain: intercom,
            uptime_sec: 0,
            wifi_ssid: None,
            wifi_rssi_dbm: None,
            ip: None,
            mac: [0; 6],
            active_slot: "ota_0",
            slot_valid: true,
            free_heap_bytes: 0,
            total_heap_bytes: 0,
            available_ecosystems: &[],
            active_ecosystem: None,
            version: "0.1.0",
            git_commit: "test",
            build_date: "test",
        };

        // Assert
        assert_eq!(diagnostics.domain, intercom);
        assert!(diagnostics.domain.door_locked);
        assert_eq!(diagnostics.domain.door_unlock_duration_sec, 5);
    }

    #[test]
    fn telemetry_snapshot_tracks_bus_and_call_state() {
        // Arrange
        let expected_bus = super::BusLineState::Talk;
        let expected_call = super::CallState::Talking;

        // Act
        super::update_bus_state(expected_bus);
        super::update_call_state(expected_call);
        let snapshot = super::current_intercom_state();
        super::update_bus_state(super::BusLineState::Idle);
        super::update_call_state(super::CallState::Idle);

        // Assert
        assert_eq!(snapshot.bus_state, expected_bus);
        assert_eq!(snapshot.call_state, expected_call);
    }
}
