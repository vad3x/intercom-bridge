//! Smart Intercom - System Reset Service

pub use edf_core::service::ResetService;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{ResetScope, ResetTriggerDriver, SystemResetDriver};

    struct DummyTrigger;

    impl ResetTriggerDriver for DummyTrigger {
        async fn wait_for_reset(&mut self) -> ResetScope {
            ResetScope::Factory
        }
    }

    struct DummyReset;

    impl SystemResetDriver for DummyReset {
        fn erase_credentials(&self, _scope: ResetScope) {}

        fn reboot(&self) -> ! {
            panic!("mock reboot")
        }
    }

    #[test]
    fn consumer_reset_service_is_the_foundation_service() {
        // Arrange
        let service = ResetService::new(DummyTrigger, DummyReset);

        // Act
        let shared: edf_core::service::ResetService<DummyTrigger, DummyReset> = service;

        // Assert
        // Compilation proves both module paths name the same generic service type.
        assert!(core::mem::size_of_val(&shared) > 0);
    }
}
