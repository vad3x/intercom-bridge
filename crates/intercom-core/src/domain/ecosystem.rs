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
