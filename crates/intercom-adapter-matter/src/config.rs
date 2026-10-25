//! Smart Intercom - Matter Adapter Configuration
//!
//! Protocol-specific endpoint IDs, device serial formatting, and constants
//! defined by the Matter Specification.

use intercom_core::domain::EcosystemId;

// ============================================================================
// Ecosystem Identity
// ============================================================================

/// Strongly-typed identifier for the Matter smart home ecosystem.
pub const MATTER_ECOSYSTEM: EcosystemId = EcosystemId::new("Matter");

// ============================================================================
// Matter Endpoints
// ============================================================================

/// Matter endpoint ID for the door lock cluster
pub const DOOR_LOCK_ENDPOINT_ID: u16 = 1;

/// Matter endpoint ID for the occupancy sensor cluster
pub const OCCUPANCY_SENSOR_ENDPOINT_ID: u16 = 2;

// ============================================================================
// Subscription Interval Policies (Matter Spec 8.5.1)
// ============================================================================

/// Maximum reporting interval granted to Matter subscriptions in seconds.
///
/// Apple Home requests 600s (10 min). Clamping this to 120s (2 min) forces the
/// Apple Home Hub to check in and re-establish CASE within 2 minutes of an ESP32 reboot.
pub const MAX_SUBSCRIPTION_INTERVAL_SECS: u16 = 120;
