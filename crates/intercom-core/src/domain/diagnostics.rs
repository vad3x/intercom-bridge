//! System diagnostics domain model.

use crate::events::SystemState;

/// Real-time health, network, flash, and firmware telemetry snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemDiagnostics {
    pub state: SystemState,
    pub door_locked: bool,
    pub uptime_sec: u64,
    pub wifi_rssi_dbm: Option<i8>,
    pub ip: Option<[u8; 4]>,
    pub mac: [u8; 6],
    pub active_slot: &'static str,
    pub slot_valid: bool,
    pub free_heap_bytes: usize,
    pub total_heap_bytes: usize,
    pub ecosystem_paired: bool,
    pub ecosystem_protocol: &'static str,
    pub version: &'static str,
    pub git_commit: &'static str,
    pub build_date: &'static str,
}
