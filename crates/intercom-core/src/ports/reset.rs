//! System reset port.
//!
//! In Hexagonal Architecture, this outbound port defines the capability
//! for erasing persistent credentials and triggering a system reboot.

/// System reset and credential management port.
pub trait SystemResetDriver {
    /// Erase persistent commissioning / pairing credentials.
    fn erase_credentials(&self);

    /// Trigger hardware software reboot.
    fn reboot(&self) -> !;
}

impl<T: SystemResetDriver> SystemResetDriver for &T {
    fn erase_credentials(&self) {
        (*self).erase_credentials();
    }

    fn reboot(&self) -> ! {
        (*self).reboot();
    }
}

impl SystemResetDriver for () {
    fn erase_credentials(&self) {}

    fn reboot(&self) -> ! {
        panic!("Reboot requested on null reset driver");
    }
}

