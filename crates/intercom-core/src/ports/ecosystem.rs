//! Outbound port traits for smart home ecosystem integrations (e.g. Matter).

/// Ecosystem pairing and commissioning status provider.
///
/// Implemented by smart home protocol adapters (e.g. `intercom-adapter-matter`).
/// Decouples the core domain and diagnostic telemetry from protocol-specific details.
pub trait EcosystemInfoDriver {
    /// Return whether the device is paired or commissioned with an ecosystem controller.
    fn is_paired(&self) -> bool;

    /// Return the name of the active smart home protocol (e.g. "Matter", "None").
    fn protocol_name(&self) -> &'static str {
        "None"
    }
}

impl EcosystemInfoDriver for () {
    fn is_paired(&self) -> bool {
        false
    }

    fn protocol_name(&self) -> &'static str {
        "None"
    }
}
