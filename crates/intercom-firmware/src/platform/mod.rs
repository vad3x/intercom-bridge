//! Hardware platform selection and lifecycle abstraction.

#[cfg(any(not(target_arch = "riscv32"), feature = "esp32c6"))]
pub mod c6;
#[cfg(not(target_arch = "riscv32"))]
pub mod mock;
#[cfg(any(not(target_arch = "riscv32"), feature = "esp32s31"))]
pub mod s31;
#[cfg(feature = "esp32s31")]
pub mod synthetic_audio;

/// Peripheral ownership transferred from platform startup to the application.
pub struct PlatformDrivers<P> {
    pub peripherals: P,
}

/// Consume the latest shared call and lifecycle state updates for an audio worker.
pub fn take_audio_state_updates(
    call_state: &mut intercom_core::events::CallStateReceiver<'_>,
    system_state: &mut intercom_core::events::SystemStateReceiver<'_>,
) -> (
    Option<intercom_core::events::SessionSnapshot>,
    Option<intercom_core::events::SystemState>,
) {
    (call_state.try_changed(), system_state.try_changed())
}

/// Peripherals shared by the C6 and S31 application wiring after platform-owned
/// timer and Core 1 peripherals have been consumed.
#[cfg(target_arch = "riscv32")]
#[allow(non_snake_case)]
pub struct PlatformPeripherals {
    pub WIFI: esp_hal::peripherals::WIFI<'static>,
    #[cfg(feature = "esp32c6")]
    pub BT: esp_hal::peripherals::BT<'static>,
    pub RNG: esp_hal::peripherals::RNG<'static>,
    pub ADC1: esp_hal::peripherals::ADC1<'static>,
    pub I2C0: esp_hal::peripherals::I2C0<'static>,
    pub GPIO0: esp_hal::peripherals::GPIO0<'static>,
    pub GPIO1: esp_hal::peripherals::GPIO1<'static>,
    pub GPIO2: esp_hal::peripherals::GPIO2<'static>,
    pub GPIO7: esp_hal::peripherals::GPIO7<'static>,
    pub GPIO8: esp_hal::peripherals::GPIO8<'static>,
    pub GPIO9: esp_hal::peripherals::GPIO9<'static>,
    pub GPIO10: esp_hal::peripherals::GPIO10<'static>,
    pub GPIO11: esp_hal::peripherals::GPIO11<'static>,
    pub GPIO15: esp_hal::peripherals::GPIO15<'static>,
    pub GPIO18: esp_hal::peripherals::GPIO18<'static>,
    pub GPIO19: esp_hal::peripherals::GPIO19<'static>,
    pub GPIO20: esp_hal::peripherals::GPIO20<'static>,
    pub GPIO21: esp_hal::peripherals::GPIO21<'static>,
    pub GPIO22: esp_hal::peripherals::GPIO22<'static>,
    pub GPIO23: esp_hal::peripherals::GPIO23<'static>,
    #[cfg(feature = "esp32c6")]
    pub I2S0: esp_hal::peripherals::I2S0<'static>,
    #[cfg(feature = "esp32c6")]
    pub DMA_CH0: esp_hal::peripherals::DMA_CH0<'static>,
    pub RMT: esp_hal::peripherals::RMT<'static>,
}

/// Initializes a hardware target and starts platform-specific worker tasks.
pub trait PlatformDriver: Sized {
    type Drivers;

    fn init() -> (Self, Self::Drivers);

    fn spawn_audio_worker(
        &mut self,
        spawner: embassy_executor::Spawner,
        worker: impl FnOnce(embassy_executor::Spawner) + Send + 'static,
    );
}

#[cfg(not(target_arch = "riscv32"))]
pub type ActivePlatform = mock::MockPlatform;

#[cfg(all(
    target_arch = "riscv32",
    feature = "esp32c6",
    not(feature = "esp32s31")
))]
pub type ActivePlatform = c6::C6Platform;

#[cfg(all(
    target_arch = "riscv32",
    feature = "esp32s31",
    not(feature = "esp32c6")
))]
pub type ActivePlatform = s31::S31Platform;

#[cfg(all(target_arch = "riscv32", feature = "esp32c6", feature = "esp32s31"))]
compile_error!("features `esp32c6` and `esp32s31` are mutually exclusive");

#[cfg(all(
    target_arch = "riscv32",
    not(any(feature = "esp32c6", feature = "esp32s31"))
))]
compile_error!("enable exactly one platform feature: `esp32c6` or `esp32s31`");

#[cfg(test)]
mod tests {
    use std::boxed::Box;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use super::{PlatformDriver, mock::MockPlatform};
    use embassy_executor::Executor;
    use intercom_core::domain::intercom::{CallState, SessionOwner};
    use intercom_core::events::{
        CallStateBroadcastReporter, CallStateWatch, SessionSnapshot, SystemState,
        SystemStateSender, SystemStateWatch,
    };
    use intercom_core::ports::IntercomReporter;

    #[test]
    fn mock_platform_initializes_and_runs_audio_worker() {
        let (mut platform, _drivers) = MockPlatform::init();
        let worker_ran = Arc::new(AtomicBool::new(false));
        let worker_ran_from_task = Arc::clone(&worker_ran);
        let executor = Box::leak(Box::new(Executor::new()));

        executor.run_until(
            |spawner| {
                platform.spawn_audio_worker(spawner, move |_worker_spawner| {
                    worker_ran_from_task.store(true, Ordering::SeqCst);
                });
            },
            || worker_ran.load(Ordering::SeqCst),
        );

        assert!(worker_ran.load(Ordering::SeqCst));
    }

    #[test]
    fn audio_worker_receives_shared_call_and_system_state_updates() {
        let call_watch = CallStateWatch::new();
        let system_watch = SystemStateWatch::new();
        let mut call_receiver = call_watch.receiver().unwrap();
        let mut system_receiver = system_watch.receiver().unwrap();

        CallStateBroadcastReporter::new(&call_watch)
            .report_session_state(CallState::Listening, Some(SessionOwner::MatterCall));
        SystemStateSender::new(&system_watch).operational();

        let (call_update, system_update) =
            super::take_audio_state_updates(&mut call_receiver, &mut system_receiver);

        assert_eq!(
            call_update,
            Some(SessionSnapshot {
                call_state: CallState::Listening,
                active_owner: Some(SessionOwner::MatterCall),
            })
        );
        assert_eq!(system_update, Some(SystemState::Operational));
    }
}
