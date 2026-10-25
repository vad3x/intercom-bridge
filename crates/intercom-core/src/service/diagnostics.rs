//! Diagnostics application service aggregating telemetry into system snapshots.

use crate::domain::diagnostics::{IntercomState, SystemDiagnostics};
use crate::domain::ecosystem::{EcosystemId, EcosystemType};
use crate::events::SystemStateWatch;
use crate::ports::diagnostics::HardwareInfoDriver;
use crate::ports::ecosystem::EcosystemInfoDriver;

/// Service responsible for producing point-in-time system diagnostics snapshots.
pub struct DiagnosticsService<
    'a,
    H: HardwareInfoDriver,
    E: EcosystemInfoDriver = (),
    T: EcosystemType = EcosystemId,
> {
    inner: edf_core::service::diagnostics::DiagnosticsService<'a, H, E, T>,
}

impl<'a, H: HardwareInfoDriver> DiagnosticsService<'a, H, (), EcosystemId> {
    /// Create a new DiagnosticsService instance without a dedicated smart home ecosystem driver.
    pub fn new(hardware: &'a H, state_watch: &'a SystemStateWatch) -> Self {
        Self {
            inner: edf_core::service::diagnostics::DiagnosticsService::new(hardware, state_watch),
        }
    }
}

impl<'a, H: HardwareInfoDriver, E: EcosystemInfoDriver, T: EcosystemType>
    DiagnosticsService<'a, H, E, T>
{
    /// Create a new DiagnosticsService instance with a dedicated smart home ecosystem driver
    /// and a slice of available ecosystems provided by the composition root.
    pub fn with_ecosystem(
        hardware: &'a H,
        ecosystem: &'a E,
        available_ecosystems: &'static [T],
        state_watch: &'a SystemStateWatch,
    ) -> Self {
        Self {
            inner: edf_core::service::diagnostics::DiagnosticsService::with_ecosystem(
                hardware,
                ecosystem,
                available_ecosystems,
                state_watch,
            ),
        }
    }

    /// Aggregate current domain state, ecosystem status, and hardware telemetry into a snapshot.
    pub fn snapshot(&self) -> SystemDiagnostics<IntercomState, T> {
        self.inner.snapshot(crate::domain::diagnostics::current_intercom_state())
    }

    /// Reference to the underlying hardware info driver.
    pub fn hardware(&self) -> &H {
        self.inner.hardware()
    }

    /// Reference to the underlying ecosystem info driver.
    pub fn ecosystem(&self) -> &E {
        self.inner.ecosystem()
    }

    /// Reference to the system state watch signal.
    pub fn state_watch(&self) -> &SystemStateWatch {
        self.inner.state_watch()
    }
}
