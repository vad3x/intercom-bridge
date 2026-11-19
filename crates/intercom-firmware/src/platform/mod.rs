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
pub fn force_bus_relays_low() {}

#[cfg(not(target_os = "none"))]
pub fn software_reset() -> ! {
    panic!("Host reboot requested")
}

#[cfg(target_os = "none")]
mod target {
    #[cfg(all(feature = "esp32c6", not(feature = "esp32s31")))]
    pub use super::c6::board as active_board;
    #[cfg(all(feature = "esp32s3", not(any(feature = "esp32c6", feature = "esp32s31"))))]
    pub use super::s3::board as active_board;
    #[cfg(all(feature = "esp32s31", not(feature = "esp32c6")))]
    pub use super::s31::board as active_board;

    pub use active_board::{
        allocate_dtls_srtp_resources, allocate_matter_stack_storage,
        allocate_webrtc_media_resources, create_webrtc_engine, force_bus_relays_low, init_heap,
        log_pre_matter_heap, wire_board, BoardAudioCtrl, BoardWireless,
    };

    pub fn software_reset() -> ! {
        esp_hal::system::software_reset();
    }

    #[cfg(not(feature = "led-smartled"))]
    pub type ActiveIndicator = intercom_adapter_gpio::led::GpioLedDriver;
    #[cfg(feature = "led-smartled")]
    pub type ActiveIndicator = crate::devkit_led::DevKitLedDriver;

    #[cfg(not(feature = "ota-disabled"))]
    pub type ActiveOtaDriver = edf_ota::FlashOtaDriver;
    #[cfg(feature = "ota-disabled")]
    pub type ActiveOtaDriver = crate::disabled_ota::DisabledOtaDriver;

    pub struct BoardHardware {
        pub wireless: BoardWireless,
        pub bus: &'static intercom_core::service::BusSafetyInterlock<
            intercom_adapter_gpio::bus::GpioBusDriver,
        >,
        pub ring: intercom_adapter_gpio::ring::GpioRingDriver,
        pub audio_ctrl: BoardAudioCtrl,
        pub indicator: &'static ActiveIndicator,
        pub boot_button: intercom_adapter_gpio::boot_button::GpioBootButtonDriver,
        pub ota_driver: ActiveOtaDriver,
    }

    pub struct HardwareSecureRandom;

    impl intercom_adapter_webrtc::signaling::SecureRandom for HardwareSecureRandom {
        fn fill(
            &self,
            out: &mut [u8],
        ) -> Result<(), intercom_adapter_webrtc::signaling::SignalingError> {
            active_board::fill_secure_random(out)
        }
    }

    pub const ENABLE_SYNTHETIC_AUDIO_STREAM: bool =
        cfg!(any(feature = "devkit", feature = "esp32s3"));

    pub fn create_default_ota_driver() -> ActiveOtaDriver {
        #[cfg(not(feature = "ota-disabled"))]
        {
            edf_ota::FlashOtaDriver::new(crate::config::OTA_PUBLIC_KEY)
        }
        #[cfg(feature = "ota-disabled")]
        {
            crate::disabled_ota::DisabledOtaDriver
        }
    }

    #[cfg(feature = "esp32s3")]
    pub fn create_s3_ota_driver() -> ActiveOtaDriver {
        #[cfg(not(feature = "ota-disabled"))]
        {
            edf_ota::FlashOtaDriver::new_with_profile(
                crate::config::OTA_PUBLIC_KEY,
                edf_ota::OtaImageProfile {
                    chip_id: edf_ota::ESP32_S3_CHIP_ID,
                    slot_size: crate::config::S3_ACTIVE_LAYOUT.app_partitions[0]
                        .expect("S3 OTA profile must define app slot 0")
                        .size as usize,
                    ota_0_offset: crate::config::S3_ACTIVE_LAYOUT.app_partitions[0]
                        .expect("S3 OTA profile must define app slot 0")
                        .offset,
                    ota_1_offset: crate::config::S3_ACTIVE_LAYOUT.app_partitions[1]
                        .expect("S3 OTA profile must define app slot 1")
                        .offset,
                },
            )
        }
        #[cfg(feature = "ota-disabled")]
        {
            crate::disabled_ota::DisabledOtaDriver
        }
    }

    pub fn mark_running_slot_valid() {
        #[cfg(not(feature = "ota-disabled"))]
        {
            if let Err(e) = edf_ota::bootloader::mark_running_slot_valid() {
                log::warn!("Failed to mark running slot valid: {:?}", e);
            } else {
                log::info!("Running firmware slot confirmed valid (rollback watchdog cancelled)");
            }
        }
        #[cfg(feature = "ota-disabled")]
        {
            log::info!("OTA rollback watchdog check skipped (OTA updates disabled on this profile)");
        }
    }

    pub fn record_and_log_media_free_heap_bytes() -> u32 {
        let low_water = crate::hardware_info::FirmwareHardwareInfo::record_media_resource_sample();
        #[cfg(feature = "esp32s3")]
        {
            static SAMPLE_COUNT: portable_atomic::AtomicU32 = portable_atomic::AtomicU32::new(0);
            let sample = SAMPLE_COUNT.fetch_add(1, portable_atomic::Ordering::Relaxed);
            if sample % 50 == 0 {
                log::info!(
                    "WebRTC media resource low water min_internal_free_bytes={} min_audio_stack_free_bytes={} latest_psram_free_bytes={}",
                    low_water.min_internal_free_bytes,
                    low_water.min_audio_stack_free_bytes,
                    low_water.latest_psram_free_bytes,
                );
            }
        }
        low_water.min_internal_free_bytes.min(u32::MAX as usize) as u32
    }

    #[cfg(any(all(feature = "esp32c6", not(feature = "devkit")), feature = "esp32s3"))]
    #[embassy_executor::task]
    pub async fn i2s_audio_worker(
        pipeline: intercom_adapter_audio::I2sAudioPipeline,
        rx_audio: &'static intercom_media::RxAudioChannel,
        tx_audio: &'static intercom_media::TxAudioChannel,
        capture_source: intercom_adapter_audio::I2sCaptureSource,
    ) {
        if let Err(error) = pipeline
            .run_with_capture_source(rx_audio.sender(), tx_audio.receiver(), capture_source)
            .await
        {
            log::warn!("I2S DMA stream failed: {:?}", error);
            software_reset();
        }
    }

    #[cfg(any(feature = "esp32c6", feature = "esp32s3"))]
    pub async fn run_esp_coex_or_oper_stack<'a, C, H, S, const KB: usize, M>(
        wifi: esp_hal::peripherals::WIFI<'static>,
        bt: esp_hal::peripherals::BT<'static>,
        stack: &'a intercom_adapter_matter::MatterStackStorage,
        weak_rand: C::WeakRand<'a>,
        hostname: &'a str,
        is_commissioned: bool,
        crypto: &'a C,
        handler: (rs_matter::dm::Node<'a>, H),
        kv: rs_matter::dm::persist::SharedKvBlobStore<'a, S, KB>,
        monitor: M,
    ) where
        C: rs_matter::crypto::Crypto,
        H: rs_matter::dm::AsyncHandler,
        S: rs_matter::persist::KvBlobStore,
        M: rs_matter_embassy::stack::UserTask,
    {
        let wifi = rs_matter_embassy::wireless::EmbassyWifi::new(
            rs_matter_embassy::wireless::esp::EspWifiRadio::new(wifi, bt),
            weak_rand,
            true,
            stack,
        )
        .with_hostname(hostname);

        match intercom_adapter_matter::commissioning::select_wireless_boot_mode(is_commissioned) {
            intercom_adapter_matter::commissioning::WirelessBootMode::OperationalWifiOnly => {
                log::info!("Commissioned fabric present -> starting Wi-Fi-only operational stack");
                stack
                    .run_wifi_oper(wifi, crypto, handler, kv, monitor)
                    .await
                    .expect("Matter operational Wi-Fi stack fatal error");
            }
            intercom_adapter_matter::commissioning::WirelessBootMode::BleWifiCoex => {
                log::info!("Uncommissioned -> starting concurrent BLE + Wi-Fi commissioning");
                stack
                    .run_wifi(wifi, crypto, handler, kv, monitor)
                    .await
                    .expect("Matter coex stack fatal error");
            }
        }
    }

    pub async fn run_bench_audio_input(rx_audio: &'static intercom_media::RxAudioChannel) {
        #[cfg(any(feature = "devkit", feature = "esp32s3"))]
        {
            let mut speech = intercom_media::Pcm16LeLoopingSource::new(
                include_bytes!("../../assets/devkit_test_music.pcm"),
                0,
            )
            .expect("embedded DevKit speech sample must contain complete PCM samples");
            let mut schedule =
                intercom_media::AudioCaptureSchedule::new(embassy_time::Instant::now().as_millis());
            let mut drop_window = intercom_media::AudioFrameDropWindow::new();
            let mut simulated_dma_overruns = 0u64;
            let levels = intercom_media::compute_frame_levels_dbfs(&speech.next_frame());
            intercom_core::domain::diagnostics::update_rx_levels_dbfs(
                levels.rms_dbfs,
                levels.peak_dbfs,
            );
            log::info!("Test audio input: embedded 16 kHz mono speech sample, continuous loop");
            loop {
                let now_ms = embassy_time::Instant::now().as_millis();
                embassy_time::Timer::after_millis(schedule.delay_until_next_ms(now_ms)).await;
                let batch = schedule.take_due_frames(embassy_time::Instant::now().as_millis());
                simulated_dma_overruns = simulated_dma_overruns.saturating_add(batch.overruns);

                for _ in 0..batch.frames {
                    let queue_dropped = rx_audio.try_send(speech.next_frame()).is_err();
                    if let Some(queue_drops) = drop_window.record(queue_dropped) {
                        log::debug!(
                            "Test audio frames=50 simulated_overruns={} queue_drops={}",
                            simulated_dma_overruns, queue_drops,
                        );
                        simulated_dma_overruns = 0;
                    }
                }
            }
        }
        #[cfg(not(any(feature = "devkit", feature = "esp32s3")))]
        {
            let _ = rx_audio;
            core::future::pending::<()>().await;
        }
    }
}

#[cfg(target_os = "none")]
pub use target::*;

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

/// Minimum untouched bytes observed on the Core 1 audio stack (0 on single-core targets).
pub fn audio_stack_free_low_water_bytes() -> usize {
    #[cfg(all(target_os = "none", feature = "esp32s3"))]
    {
        s3::audio_stack_free_low_water_bytes()
    }
    #[cfg(not(all(target_os = "none", feature = "esp32s3")))]
    {
        0
    }
}

/// Query the connected or configured Wi-Fi station SSID from the active chip's radio bindings.
pub fn wifi_ssid() -> Option<intercom_core::domain::diagnostics::WifiSsid> {
    #[cfg(all(target_os = "none", any(feature = "esp32c6", feature = "esp32s3")))]
    {
        #[cfg(feature = "esp32c6")]
        use esp_wifi_sys_esp32c6::include as wifi_sys;
        #[cfg(feature = "esp32s3")]
        use esp_wifi_sys_esp32s3::include as wifi_sys;

        let mut record = core::mem::MaybeUninit::<wifi_sys::wifi_ap_record_t>::uninit();
        if unsafe { wifi_sys::esp_wifi_sta_get_ap_info(record.as_mut_ptr()) }
            == wifi_sys::ESP_OK as i32
        {
            let record = unsafe { record.assume_init() };
            let len = record
                .ssid
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(record.ssid.len());
            if len > 0 {
                if let Some(ssid) =
                    intercom_core::domain::diagnostics::WifiSsid::from_bytes(&record.ssid[..len])
                {
                    return Some(ssid);
                }
            }
        }

        let mut cfg = core::mem::MaybeUninit::<wifi_sys::wifi_config_t>::uninit();
        if unsafe {
            wifi_sys::esp_wifi_get_config(wifi_sys::wifi_interface_t_WIFI_IF_STA, cfg.as_mut_ptr())
        } == wifi_sys::ESP_OK as i32
        {
            let cfg = unsafe { cfg.assume_init() };
            let ssid_bytes = unsafe { cfg.sta.ssid };
            let len = ssid_bytes
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(ssid_bytes.len());
            if len > 0 {
                if let Some(ssid) =
                    intercom_core::domain::diagnostics::WifiSsid::from_bytes(&ssid_bytes[..len])
                {
                    return Some(ssid);
                }
            }
        }
    }
    None
}

/// Query the active Wi-Fi station RSSI in dBm.
pub fn wifi_rssi() -> Option<i8> {
    #[cfg(all(target_os = "none", any(feature = "esp32c6", feature = "esp32s3")))]
    {
        extern "C" {
            fn esp_wifi_sta_get_rssi(rssi: *mut i32) -> i32;
        }
        let mut rssi: i32 = 0;
        if unsafe { esp_wifi_sta_get_rssi(&mut rssi) } == 0 {
            return Some(rssi as i8);
        }
    }
    None
}

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
