//! Smart home ecosystem domain model.
//!
//! Protocol-agnostic value objects and status representations for smart home ecosystems.

/// Maximum number of free-form key-value telemetry properties per ecosystem.
pub const MAX_ECOSYSTEM_PROPERTIES: usize = 6;

/// Strongly-typed value object identifying a smart home ecosystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EcosystemId(&'static str);

impl EcosystemId {
    /// Create a new `EcosystemId` from a static string identifier.
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    /// Return the underlying string slice identifier.
    pub const fn as_str(&self) -> &'static str {
        self.0
    }
}

/// Trait implemented by composition-root ecosystem enums to map variants to [`EcosystemId`].
pub trait EcosystemType: Copy + Eq + core::fmt::Debug + 'static {
    /// Return the strongly-typed [`EcosystemId`] for this ecosystem variant.
    fn id(&self) -> EcosystemId;
}

impl EcosystemType for EcosystemId {
    fn id(&self) -> EcosystemId {
        *self
    }
}

/// Typed, zero-allocation value for per-ecosystem telemetry properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EcosystemValue {
    Bool(bool),
    Int(i64),
    Text(&'static str),
}

/// Maximum number of active smart home fabrics/hubs tracked in telemetry.
pub const MAX_ECOSYSTEM_FABRICS: usize = 5;

/// Fixed-capacity 32-byte string buffer for fabric labels, implementing Copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FabricLabel {
    bytes: [u8; 32],
    len: u8,
}

impl FabricLabel {
    /// Create an empty fabric label.
    pub const fn empty() -> Self {
        Self {
            bytes: [0; 32],
            len: 0,
        }
    }

    /// Create a fabric label from a string slice, truncating to 32 bytes if necessary.
    pub fn from_str(s: &str) -> Self {
        let mut bytes = [0u8; 32];
        let copy_len = core::cmp::min(s.len(), 32);
        bytes[..copy_len].copy_from_slice(&s.as_bytes()[..copy_len]);
        Self {
            bytes,
            len: copy_len as u8,
        }
    }

    /// Return the label as a string slice.
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }
}

/// Telemetry snapshot for an individual connected smart home fabric / hub.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FabricInfo {
    /// 1-based fabric index within the device's fabric table (1..=254)
    pub fab_idx: u8,
    /// Vendor ID of the commissioner or ecosystem (e.g. 0x1349, 0x1384, 0x130D)
    pub vendor_id: u16,
    /// 64-bit Fabric ID assigned by the operational CA
    pub fabric_id: u64,
    /// 64-bit Node ID assigned to this accessory on the fabric
    pub node_id: u64,
    /// Human-readable label configured by user or ecosystem (up to 32 bytes)
    pub label: FabricLabel,
}

/// Maximum number of exposed smart home endpoints.
pub const MAX_ECOSYSTEM_ENDPOINTS: usize = 4;

/// Snapshot descriptor for an exposed smart home endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointInfo {
    /// Numerical endpoint ID (e.g. 1, 2)
    pub id: u16,
    /// Human-readable name (e.g. "Door Lock", "Chime / Ring")
    pub name: &'static str,
    /// Standard cluster or type description (e.g. "DoorLock", "OccupancySensing")
    pub cluster: &'static str,
    /// Whether the endpoint is currently primed / ready / subscribed
    pub ready: bool,
}

/// Status and telemetry snapshot of a single active smart home ecosystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EcosystemStatus {
    pub ecosystem_type: EcosystemId,
    pub paired: bool,
    pub properties: [Option<(&'static str, EcosystemValue)>; MAX_ECOSYSTEM_PROPERTIES],
    pub fabrics: [Option<FabricInfo>; MAX_ECOSYSTEM_FABRICS],
    pub endpoints: [Option<EndpointInfo>; MAX_ECOSYSTEM_ENDPOINTS],
}

impl EcosystemStatus {
    /// Create a new `EcosystemStatus` with no extra properties, fabrics, or endpoints.
    pub const fn new(ecosystem_type: EcosystemId, paired: bool) -> Self {
        Self {
            ecosystem_type,
            paired,
            properties: [None; MAX_ECOSYSTEM_PROPERTIES],
            fabrics: [None; MAX_ECOSYSTEM_FABRICS],
            endpoints: [None; MAX_ECOSYSTEM_ENDPOINTS],
        }
    }

    /// Add a connected fabric / hub to the telemetry snapshot.
    pub fn with_fabric(mut self, fabric: FabricInfo) -> Self {
        for slot in self.fabrics.iter_mut() {
            if slot.is_none() {
                *slot = Some(fabric);
                break;
            }
        }
        self
    }

    /// Add an exposed endpoint descriptor to the telemetry snapshot.
    pub fn with_endpoint(mut self, endpoint: EndpointInfo) -> Self {
        for slot in self.endpoints.iter_mut() {
            if slot.is_none() {
                *slot = Some(endpoint);
                break;
            }
        }
        self
    }

    /// Iterate over all active connected fabrics in insertion order.
    pub fn iter_fabrics(&self) -> impl Iterator<Item = &FabricInfo> {
        self.fabrics.iter().filter_map(|slot| slot.as_ref())
    }

    /// Iterate over all exposed endpoints in insertion order.
    pub fn iter_endpoints(&self) -> impl Iterator<Item = &EndpointInfo> {
        self.endpoints.iter().filter_map(|slot| slot.as_ref())
    }

    /// Builder method to set or update a key-value property.
    ///
    /// If `key` already exists in `self.properties`, its value is updated;
    /// otherwise it is inserted into the first empty slot.
    pub const fn with_property(mut self, key: &'static str, value: EcosystemValue) -> Self {
        let mut i = 0;
        let mut first_none = None;
        while i < MAX_ECOSYSTEM_PROPERTIES {
            if let Some((k, _)) = self.properties[i] {
                if str_eq(k, key) {
                    self.properties[i] = Some((key, value));
                    return self;
                }
            } else if first_none.is_none() {
                first_none = Some(i);
            }
            i += 1;
        }
        if let Some(slot) = first_none {
            self.properties[slot] = Some((key, value));
        }
        self
    }

    /// Return the value of the property with the given key, if present.
    pub fn property(&self, key: &str) -> Option<&EcosystemValue> {
        self.properties.iter().find_map(|prop| match prop {
            Some((k, v)) if *k == key => Some(v),
            _ => None,
        })
    }

    /// Iterate over all populated `(key, value)` properties in insertion order.
    pub fn iter_properties(&self) -> impl Iterator<Item = (&'static str, EcosystemValue)> + '_ {
        self.properties.iter().filter_map(|prop| *prop)
    }
}

const fn str_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}
