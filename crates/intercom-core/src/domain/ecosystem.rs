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

/// Status and telemetry snapshot of a single active smart home ecosystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EcosystemStatus {
    pub ecosystem_type: EcosystemId,
    pub paired: bool,
    pub properties: [Option<(&'static str, EcosystemValue)>; MAX_ECOSYSTEM_PROPERTIES],
}

impl EcosystemStatus {
    /// Create a new `EcosystemStatus` with no extra properties.
    pub const fn new(ecosystem_type: EcosystemId, paired: bool) -> Self {
        Self {
            ecosystem_type,
            paired,
            properties: [None; MAX_ECOSYSTEM_PROPERTIES],
        }
    }

    /// Builder method to append a key-value property into the next available slot.
    pub const fn with_property(mut self, key: &'static str, value: EcosystemValue) -> Self {
        let mut i = 0;
        while i < MAX_ECOSYSTEM_PROPERTIES {
            if self.properties[i].is_none() {
                self.properties[i] = Some((key, value));
                break;
            }
            i += 1;
        }
        self
    }

    /// Iterate over all populated `(key, value)` properties in insertion order.
    pub fn iter_properties(&self) -> impl Iterator<Item = (&'static str, EcosystemValue)> + '_ {
        self.properties.iter().filter_map(|prop| *prop)
    }
}
