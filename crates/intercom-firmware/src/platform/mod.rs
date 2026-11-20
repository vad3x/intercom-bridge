//! Hardware platform selection and lifecycle abstraction.

/// A `MaybeUninit` wrapper that can reside in the bootloader-reclaimed RAM section
/// (which is not zero-initialized at startup, so it must not contain a `StaticCell` guard).
#[repr(transparent)]
pub struct ReclaimedCell<T>(core::cell::UnsafeCell<core::mem::MaybeUninit<T>>);

unsafe impl<T: Send> Sync for ReclaimedCell<T> {}

#[cfg(target_os = "none")]
// SAFETY: `ReclaimedCell` wraps `MaybeUninit<T>` behind `UnsafeCell`. Its
// constructor requires no initialized bytes, which is the reclaimed section's contract.
unsafe impl<T> esp_hal::Uninit for ReclaimedCell<T> {}

impl<T> ReclaimedCell<T> {
    pub const fn new() -> Self {
        Self(core::cell::UnsafeCell::new(core::mem::MaybeUninit::uninit()))
    }

    pub fn init(&'static self, value: T) -> &'static mut T {
        // SAFETY: callers invoke `init` once during single-threaded startup before any alias exists.
        unsafe { (*self.0.get()).write(value) }
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

/// Build an `embassy_net::DhcpConfig` with DHCPv4 Option 12 hostname when non-empty and <= 32 bytes.
pub fn dhcp_config_with_hostname(hostname: &str) -> embassy_net::DhcpConfig {
    let mut dhcp = embassy_net::DhcpConfig::default();
    if !hostname.is_empty() {
        if let Ok(parsed) = heapless::String::try_from(hostname) {
            dhcp.hostname = Some(parsed);
        }
    }
    dhcp
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

    /// Bytes reserved for the Matter stack's bump allocator.
    const MATTER_BUMP_SIZE: usize;

    fn init() -> (Self, Self::Drivers);

    fn force_bus_relays_low();

    fn software_reset() -> !;

    fn spawn_audio_worker(
        &mut self,
        spawner: embassy_executor::Spawner,
        worker: impl FnOnce(embassy_executor::Spawner) + Send + 'static,
    );
}

#[cfg(target_os = "none")]
mod target {
    #[cfg(all(feature = "esp32c6", not(feature = "esp32s31")))]
    pub use super::c6::board as active_board;
    #[cfg(all(
        feature = "esp32s3",
        not(any(feature = "esp32c6", feature = "esp32s31"))
    ))]
    pub use super::s3::board as active_board;
    #[cfg(all(feature = "esp32s31", not(feature = "esp32c6")))]
    pub use super::s31::board as active_board;

    pub use active_board::{
        allocate_dtls_srtp_resources, allocate_enet_resources, allocate_matter_stack_storage,
        allocate_webrtc_media_resources, create_webrtc_engine, log_pre_matter_heap, wire_board,
        BoardAudioCtrl, BoardWireless,
    };

    #[cfg(not(feature = "led-smartled"))]
    pub type ActiveIndicator = intercom_adapter_gpio::led::GpioLedDriver;
    #[cfg(feature = "led-smartled")]
    pub type ActiveIndicator = crate::devkit_led::DevKitLedDriver;

    #[cfg(all(not(feature = "ota-disabled"), not(feature = "esp32s3")))]
    pub type ActiveOtaDriver = edf_ota::FlashOtaDriver;
    #[cfg(all(not(feature = "ota-disabled"), feature = "esp32s3"))]
    pub type ActiveOtaDriver = edf_ota::PsramOtaDriver;
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

    #[cfg(not(feature = "esp32s3"))]
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
            edf_ota::PsramOtaDriver::new(
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
            log::info!(
                "OTA rollback watchdog check skipped (OTA updates disabled on this profile)"
            );
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
            <super::ActivePlatform as super::PlatformDriver>::software_reset();
        }
    }

    /// Pre-allocated `embassy-net` and Matter transport buffers owned by the application.
    pub struct EnetMatterBuffers {
        pub udp_buffers: &'static rs_matter_embassy::enet::EnetMatterUdpBuffers,
        pub tcp_buffers: &'static rs_matter_embassy::enet::EnetMatterTcpBuffers,
        pub matter_tcp_buffers: &'static rs_matter_embassy::enet::EnetMatterTcpTransportBuffers,
        pub tcp_config: &'static rs_matter::transport::network::tcp::TcpConfig,
        #[cfg(any(feature = "esp32c6", feature = "esp32s3"))]
        pub btp_gatt: &'static rs_matter_embassy::ble::BtpGattContext,
    }

    pub struct EnetResources {
        pub stack_resources: &'static mut rs_matter_embassy::enet::EnetMatterStackResources,
        pub buffers: EnetMatterBuffers,
    }

    /// Initialize the `esp-radio` Wi-Fi controller and `embassy-net` stack once for the application.
    pub fn init_esp_network(
        wifi: esp_hal::peripherals::WIFI<'static>,
        hostname: &str,
        seed: u64,
        stack_resources: &'static mut rs_matter_embassy::enet::EnetMatterStackResources,
    ) -> (
        esp_radio::wifi::WifiController<'static>,
        rs_matter_embassy::enet::net::Stack<'static>,
        rs_matter_embassy::enet::net::Runner<
            'static,
            rs_matter_embassy::enet::MatterEnetDriver<esp_radio::wifi::Interface>,
        >,
    ) {
        let mut controller = esp_radio::wifi::WifiController::new(
            wifi,
            esp_radio::wifi::ControllerConfig::default().with_sta_disconnected_pm(false),
        )
        .expect("Failed to create Wi-Fi controller");
        controller
            .set_power_saving(esp_radio::wifi::PowerSaveMode::None)
            .expect("Failed to disable Wi-Fi power saving");
        controller
            .set_max_tx_power(80)
            .expect("Failed to set Wi-Fi transmit power");

        let (stack, runner) = rs_matter_embassy::enet::create_enet_stack(
            esp_radio::wifi::Interface::station(),
            seed,
            stack_resources,
        );
        stack.set_config_v4(embassy_net::ConfigV4::Dhcp(
            super::dhcp_config_with_hostname(hostname),
        ));

        (controller, stack, runner)
    }

    #[cfg(any(feature = "esp32c6", feature = "esp32s3"))]
    #[allow(clippy::too_many_arguments)]
    pub async fn run_esp_preexisting_stack<
        'a,
        const BUMP_SIZE: usize,
        C,
        H,
        S,
        const KB: usize,
        M,
    >(
        wifi_controller: esp_radio::wifi::WifiController<'static>,
        bt: esp_hal::peripherals::BT<'static>,
        enet_stack: rs_matter_embassy::enet::net::Stack<'static>,
        enet_buffers: EnetMatterBuffers,
        stack: &'a intercom_adapter_matter::MatterStackStorage<BUMP_SIZE>,
        weak_rand: C::WeakRand<'a>,
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
        let net_stack = rs_matter_embassy::enet::EnetStack::new_with_tcp(
            enet_stack,
            enet_buffers.udp_buffers,
            enet_buffers.tcp_buffers,
            enet_buffers.matter_tcp_buffers,
            enet_buffers.tcp_config,
        );
        let netif = rs_matter_embassy::enet::EnetNetif::new(
            enet_stack,
            rs_matter_embassy::matter::dm::clusters::gen_diag::InterfaceTypeEnum::WiFi,
        );
        let net_ctl = rs_matter_embassy::wifi::esp::EspWifiController::new(wifi_controller);
        #[cfg(feature = "esp32c6")]
        let net_ctl = net_ctl.with_protocols(
            rs_matter_embassy::wireless::esp::Protocols::default().with_2_4(
                rs_matter_embassy::wireless::esp::Protocol::B
                    | rs_matter_embassy::wireless::esp::Protocol::G
                    | rs_matter_embassy::wireless::esp::Protocol::N
                    | rs_matter_embassy::wireless::esp::Protocol::AX,
            ),
        );
        let mdns = rs_matter_embassy::matter::transport::network::mdns::builtin::BuiltinMdns::new();
        let gatt = rs_matter_embassy::wireless::esp::EspGattPeripheral::new(
            bt,
            Some(weak_rand),
            enet_buffers.btp_gatt,
        );
        let wireless = rs_matter_embassy::stack::wireless::PreexistingWireless::new(
            net_stack, netif, net_ctl, mdns, gatt,
        );

        match intercom_adapter_matter::commissioning::select_wireless_boot_mode(is_commissioned) {
            intercom_adapter_matter::commissioning::WirelessBootMode::OperationalWifiOnly => {
                log::info!("Commissioned fabric present -> starting Wi-Fi-only operational stack");
                stack
                    .run_wifi_oper(wireless, crypto, handler, kv, monitor)
                    .await
                    .expect("Matter operational Wi-Fi stack fatal error");
            }
            intercom_adapter_matter::commissioning::WirelessBootMode::BleWifiCoex => {
                log::info!("Uncommissioned -> starting concurrent BLE + Wi-Fi commissioning");
                stack
                    .run_wifi(wireless, crypto, handler, kv, monitor)
                    .await
                    .expect("Matter coex stack fatal error");
            }
        }
    }

    #[cfg(all(
        feature = "bench-talk-loopback",
        any(feature = "devkit", feature = "esp32s3")
    ))]
    mod bench_loopback {
        use core::cell::RefCell;

        use embassy_sync::blocking_mutex::{raw::CriticalSectionRawMutex, Mutex};

        const MAX_LOOPBACK_FRAMES: usize = 500; // 10 seconds @ 20 ms/frame

        struct Storage {
            state: intercom_media::BenchLoopbackState,
            frames: alloc::vec::Vec<intercom_media::PcmFrame>,
            was_recording: bool,
        }

        static STORAGE: Mutex<CriticalSectionRawMutex, RefCell<Option<Storage>>> =
            Mutex::new(RefCell::new(None));

        pub fn init() {
            let frames = alloc::vec![intercom_media::PcmFrame::SILENCE; MAX_LOOPBACK_FRAMES];
            STORAGE.lock(|cell| {
                *cell.borrow_mut() = Some(Storage {
                    state: intercom_media::BenchLoopbackState::new(),
                    frames,
                    was_recording: false,
                });
            });
            intercom_media::set_bench_tx_tap(Some(on_tx_frame));
            intercom_media::set_bench_loopback_reader(Some(read_frame));
            log::info!(
                "Bench talk loopback enabled: {} frames ({} s max)",
                MAX_LOOPBACK_FRAMES,
                MAX_LOOPBACK_FRAMES / 50
            );
        }

        fn read_frame(index: usize) -> (usize, Option<intercom_media::PcmFrame>) {
            STORAGE.lock(|cell| {
                let guard = cell.borrow();
                let Some(storage) = guard.as_ref() else {
                    return (0, None);
                };
                let len = storage.state.recorded_frames().min(storage.frames.len());
                let frame = (index < len).then(|| storage.frames[index]);
                (len, frame)
            })
        }

        fn on_tx_frame(frame: &intercom_media::PcmFrame) {
            let now_ms = embassy_time::Instant::now().as_millis();
            STORAGE.lock(|cell| {
                let mut guard = cell.borrow_mut();
                let Some(storage) = guard.as_mut() else {
                    return;
                };
                storage
                    .state
                    .push_tx_frame(&mut storage.frames, frame, now_ms);
                storage.was_recording = true;
            });
        }

        pub fn next_playback_frame(now_ms: u64) -> Option<intercom_media::PcmFrame> {
            STORAGE.lock(|cell| {
                let mut guard = cell.borrow_mut();
                let storage = guard.as_mut()?;
                let Storage {
                    state,
                    frames,
                    was_recording,
                } = storage;
                let frame = state.poll_playback_frame(frames, now_ms);
                if !state.is_recording() && *was_recording {
                    *was_recording = false;
                    log::info!(
                        "Bench talk loopback: looping {} recorded frames ({} ms)",
                        state.recorded_frames(),
                        state.recorded_frames() * 20
                    );
                }
                frame
            })
        }
    }

    pub async fn run_bench_audio_input(rx_audio: &'static intercom_media::RxAudioChannel) {
        #[cfg(any(feature = "devkit", feature = "esp32s3"))]
        {
            #[cfg(feature = "bench-talk-loopback")]
            bench_loopback::init();

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
                    #[cfg(feature = "bench-talk-loopback")]
                    let frame = bench_loopback::next_playback_frame(
                        embassy_time::Instant::now().as_millis(),
                    )
                    .unwrap_or_else(|| speech.next_frame());
                    #[cfg(not(feature = "bench-talk-loopback"))]
                    let frame = speech.next_frame();

                    let levels = intercom_media::compute_frame_levels_dbfs(&frame);
                    intercom_core::domain::diagnostics::update_rx_levels_dbfs(
                        levels.rms_dbfs,
                        levels.peak_dbfs,
                    );
                    let queue_dropped = rx_audio.try_send(frame).is_err();
                    if let Some(queue_drops) = drop_window.record(queue_dropped) {
                        log::debug!(
                            "Test audio frames=50 simulated_overruns={} queue_drops={}",
                            simulated_dma_overruns,
                            queue_drops,
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
    fn dhcp_config_with_hostname_sets_valid_names_and_ignores_invalid() {
        let valid = super::dhcp_config_with_hostname("intercom-bridge");
        assert_eq!(valid.hostname.as_deref(), Some("intercom-bridge"));

        let empty = super::dhcp_config_with_hostname("");
        assert_eq!(empty.hostname, None);

        let oversized = super::dhcp_config_with_hostname("intercom-bridge-hostname-over-32-bytes");
        assert_eq!(oversized.hostname, None);
    }

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
