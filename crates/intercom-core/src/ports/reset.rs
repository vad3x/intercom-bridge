//! System reset domain port and scope definitions.

pub use crate::domain::reset::ResetScope;

/// Outbound port for performing system credential erasure and rebooting the hardware.
pub trait SystemResetDriver {
    /// Erases persistent credentials according to the specified scope.
    fn erase_credentials(&self, scope: ResetScope);

    /// Reboots the hardware controller.
    fn reboot(&self) -> !;
}

impl<T: SystemResetDriver> SystemResetDriver for &T {
    fn erase_credentials(&self, scope: ResetScope) {
        (*self).erase_credentials(scope);
    }

    fn reboot(&self) -> ! {
        (*self).reboot();
    }
}

impl SystemResetDriver for () {
    fn erase_credentials(&self, _scope: ResetScope) {}

    fn reboot(&self) -> ! {
        panic!("Reboot requested on null reset driver");
    }
}
