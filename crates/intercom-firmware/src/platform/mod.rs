//! Hardware platform selection and lifecycle abstraction.

/// A `StaticCell` wrapper that can reside in the bootloader-reclaimed RAM section.
#[repr(transparent)]
pub struct ReclaimedCell<T>(static_cell::StaticCell<T>);

#[cfg(target_os = "none")]
// SAFETY: StaticCell stores its value in MaybeUninit behind UnsafeCell. Its
// constructor requires no initialized bytes, which is the reclaimed section's contract.
unsafe impl<T> esp_hal::Uninit for ReclaimedCell<T> {}

impl<T> ReclaimedCell<T> {
    pub const fn new() -> Self {
        Self(static_cell::StaticCell::new())
    }

    pub fn init(&'static self, value: T) -> &'static mut T {
        self.0.init(value)
    }
}

#[cfg(feature = "esp32s3")]
pub mod audio_startup;
#[cfg(feature = "esp32s3")]
pub mod entropy;
pub mod target_selection;

#[cfg(any(not(target_os = "none"), feature = "esp32c6"))]
pub mod c6;
#[cfg(not(target_os = "none"))]
pub mod mock;
#[cfg(any(not(target_os = "none"), feature = "esp32s3"))]
pub mod s3;
#[cfg(any(not(target_os = "none"), feature = "esp32s31"))]
pub mod s31;
#[cfg(feature = "esp32s31")]
pub mod synthetic_audio;

#[cfg(all(
    feature = "esp32s3",
    feature = "flash-4mb",
    not(feature = "ota-disabled")
))]
compile_error!("the S3 4 MiB factory-only profile requires `ota-disabled`");

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
#[cfg(target_os = "none")]
#[allow(non_snake_case)]
pub struct PlatformPeripherals {
    pub WIFI: esp_hal::peripherals::WIFI<'static>,
    #[cfg(any(feature = "esp32c6", feature = "esp32s3"))]
    pub BT: esp_hal::peripherals::BT<'static>,
    #[cfg(not(feature = "esp32s3"))]
    pub RNG: esp_hal::peripherals::RNG<'static>,
    #[cfg(not(feature = "esp32s3"))]
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
    #[cfg(any(feature = "esp32c6", feature = "esp32s31"))]
    pub GPIO22: esp_hal::peripherals::GPIO22<'static>,
    #[cfg(any(feature = "esp32c6", feature = "esp32s31"))]
    pub GPIO23: esp_hal::peripherals::GPIO23<'static>,
    #[cfg(any(feature = "esp32c6", feature = "esp32s3"))]
    pub I2S0: esp_hal::peripherals::I2S0<'static>,
    #[cfg(any(feature = "esp32c6", feature = "esp32s3"))]
    pub DMA_CH0: esp_hal::peripherals::DMA_CH0<'static>,
    #[cfg(feature = "esp32s3")]
    pub GPIO4: esp_hal::peripherals::GPIO4<'static>,
    #[cfg(feature = "esp32s3")]
    pub GPIO12: esp_hal::peripherals::GPIO12<'static>,
    #[cfg(feature = "esp32s3")]
    pub GPIO13: esp_hal::peripherals::GPIO13<'static>,
    #[cfg(feature = "esp32s3")]
    pub GPIO14: esp_hal::peripherals::GPIO14<'static>,
    #[cfg(feature = "esp32s3")]
    pub GPIO16: esp_hal::peripherals::GPIO16<'static>,
    #[cfg(feature = "esp32s3")]
    pub GPIO17: esp_hal::peripherals::GPIO17<'static>,
    #[cfg(feature = "esp32s3")]
    pub GPIO40: esp_hal::peripherals::GPIO40<'static>,
    #[cfg(feature = "esp32s3")]
    pub GPIO47: esp_hal::peripherals::GPIO47<'static>,
    #[cfg(feature = "esp32s3")]
    pub GPIO48: esp_hal::peripherals::GPIO48<'static>,
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

#[cfg(not(target_os = "none"))]
pub type ActivePlatform = mock::MockPlatform;

#[cfg(all(target_os = "none", feature = "esp32c6", not(feature = "esp32s31")))]
pub type ActivePlatform = c6::C6Platform;

#[cfg(all(target_os = "none", feature = "esp32s31", not(feature = "esp32c6")))]
pub type ActivePlatform = s31::S31Platform;

#[cfg(all(
    target_os = "none",
    feature = "esp32s3",
    not(any(feature = "esp32c6", feature = "esp32s31"))
))]
pub type ActivePlatform = s3::S3Platform;

#[cfg(all(target_os = "none", feature = "esp32c6", feature = "esp32s31"))]
compile_error!("chip features `esp32c6`, `esp32s31`, and `esp32s3` are mutually exclusive");

#[cfg(all(
    target_os = "none",
    not(any(feature = "esp32c6", feature = "esp32s31", feature = "esp32s3"))
))]
compile_error!("enable exactly one platform feature: `esp32c6`, `esp32s31`, or `esp32s3`");

#[cfg(all(target_os = "none", feature = "esp32c6", feature = "esp32s3"))]
compile_error!("chip features `esp32c6`, `esp32s31`, and `esp32s3` are mutually exclusive");

#[cfg(all(target_os = "none", feature = "esp32s31", feature = "esp32s3"))]
compile_error!("chip features `esp32c6`, `esp32s31`, and `esp32s3` are mutually exclusive");

#[cfg(test)]
mod tests {
    use std::boxed::Box;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    use super::{mock::MockPlatform, PlatformDriver, ReclaimedCell};
    use embassy_executor::Executor;
    use intercom_core::domain::intercom::{CallState, SessionOwner};
    use intercom_core::events::{
        CallStateBroadcastReporter, CallStateWatch, SessionSnapshot, SystemState,
        SystemStateSender, SystemStateWatch,
    };
    use intercom_core::ports::IntercomReporter;

    #[test]
    fn reclaimed_cell_initializes_value_in_shared_platform_module() {
        static CELL: ReclaimedCell<u32> = ReclaimedCell::new();

        assert_eq!(*CELL.init(42), 42);
    }

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
