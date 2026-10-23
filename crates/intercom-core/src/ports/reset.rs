//! System reset domain port and scope definitions.

pub use edf_core::ports::reset::{ResetScope, ResetTriggerDriver, SystemResetDriver};

#[cfg(test)]
mod tests {
    use super::*;

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

    fn requires_foundation_trigger<T: edf_core::ports::reset::ResetTriggerDriver>(_: T) {}

    #[test]
    fn consumer_trigger_trait_is_the_foundation_trait() {
        // Arrange
        let trigger = DummyTrigger;

        // Act
        requires_foundation_trigger(trigger);

        // Assert
        // Compilation proves both module paths name the same trait.
    }

    #[test]
    fn consumer_reset_trait_is_the_foundation_trait() {
        // Arrange
        let reset = DummyReset;

        // Act
        let shared: &dyn edf_core::ports::reset::SystemResetDriver = &reset;

        // Assert
        shared.erase_credentials(ResetScope::Factory);
    }
}
