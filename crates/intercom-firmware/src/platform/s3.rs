//! ESP32-S3 board profile and platform initialization.

#[cfg(target_os = "none")]
use super::{PlatformDriver, PlatformDrivers, PlatformPeripherals};

#[cfg(target_os = "none")]
const AUDIO_STACK_BYTES: usize = 32_768;

#[cfg(target_os = "none")]
#[repr(transparent)]
struct ReclaimedCell<T>(core::cell::UnsafeCell<core::mem::MaybeUninit<T>>);

#[cfg(target_os = "none")]
// SAFETY: Access is serialized by the one-time initialization guard below.
unsafe impl<T> esp_hal::Uninit for ReclaimedCell<T> {}
#[cfg(target_os = "none")]
unsafe impl<T> Sync for ReclaimedCell<T> {}

#[cfg(target_os = "none")]
static APP_CORE_STACK_INITIALIZED: portable_atomic::AtomicBool =
    portable_atomic::AtomicBool::new(false);

#[cfg(target_os = "none")]
impl<T> ReclaimedCell<T> {
    const fn new() -> Self {
        Self(core::cell::UnsafeCell::new(core::mem::MaybeUninit::uninit()))
    }

    fn init(&'static self, value: T) -> &'static mut T {
        assert!(
            APP_CORE_STACK_INITIALIZED
                .compare_exchange(
                    false,
                    true,
                    portable_atomic::Ordering::AcqRel,
                    portable_atomic::Ordering::Relaxed,
                )
                .is_ok(),
            "S3 Core 1 stack can only be initialized once"
        );
        unsafe { (*self.0.get()).write(value) }
    }
}

#[cfg(any(
    all(target_os = "none", feature = "esp32s3"),
    all(test, feature = "esp32s3")
))]
const STACK_WATERMARK: u8 = 0xA5;

#[cfg(target_os = "none")]
static AUDIO_STACK_LOW_WATER: portable_atomic::AtomicUsize =
    portable_atomic::AtomicUsize::new(usize::MAX);

#[cfg(target_os = "none")]
static AUDIO_STACK_BOTTOM: portable_atomic::AtomicUsize = portable_atomic::AtomicUsize::new(0);

#[cfg(target_os = "none")]
fn fill_radio_entropy(output: &mut [u8]) -> bool {
    esp_hal::rng::Rng::new().read(output);
    true
}

/// Return the untouched low-address prefix of a downward-growing stack.
#[cfg(any(
    all(target_os = "none", feature = "esp32s3"),
    all(test, feature = "esp32s3")
))]
fn stack_watermark_bytes(stack: &[u8]) -> usize {
    stack
        .iter()
        .take_while(|byte| **byte == STACK_WATERMARK)
        .count()
}

/// ESP32-S3 board signal assignments for the N16R8 dev board profile.
pub struct BoardPins {
    pub ring_input: u8,
    pub bus_listen: u8,
    pub boot_button: u8,
    pub bus_talk: u8,
    pub codec_reset_n: u8,
    pub i2s_mclk: u8,
    pub bus_door: u8,
    pub indicator_led: u8,
    pub i2s_bclk: u8,
    pub i2s_ws: u8,
    pub i2s_dout: u8,
    pub i2s_din: u8,
    pub codec_i2c_sda: u8,
    pub codec_i2c_scl: u8,
}

impl BoardPins {
    /// Candidate assignments avoid N16R8 memory, console, and flash pins. The
    /// supplied DevKitC pinout routes its WS2812 RGB LED to GPIO47.
    pub const SUPPORTED: Self = Self {
        ring_input: 4,
        bus_listen: 17,
        boot_button: 16,
        bus_talk: 18,
        codec_reset_n: 21,
        i2s_mclk: 10,
        bus_door: 40,
        indicator_led: 47,
        i2s_bclk: 11,
        i2s_ws: 12,
        i2s_dout: 13,
        i2s_din: 14,
        codec_i2c_sda: 8,
        codec_i2c_scl: 9,
    };

    /// Return every GPIO used by the selected board profile.
    pub const fn all_signal_pins(&self) -> [u8; 14] {
        [
            self.ring_input,
            self.bus_listen,
            self.boot_button,
            self.bus_talk,
            self.codec_reset_n,
            self.i2s_mclk,
            self.bus_door,
            self.indicator_led,
            self.i2s_bclk,
            self.i2s_ws,
            self.i2s_dout,
            self.i2s_din,
            self.codec_i2c_sda,
            self.codec_i2c_scl,
        ]
    }
}

/// Pins that the board profile must keep available for application wiring.
pub const EXPOSED_PINS: &[u8] = &[4, 8, 9, 10, 11, 12, 13, 14, 16, 17, 18, 21, 40, 47, 48];

/// GPIOs reserved for module memory, flash, USB/UART consoles, or boot strapping.
pub const RESERVED_PINS: &[u8] = &[
    0, 3, 19, 20, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 43, 44, 45, 46,
];

/// Relay outputs that must be cleared together during reset and panic handling.
pub struct RelayPins {
    pub listen: u8,
    pub talk: u8,
    pub door: u8,
}

/// GPIO bank masks for the S3 write-one-to-clear registers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpioClearMasks {
    pub gpio_low: u32,
    pub gpio_high: u32,
}

impl From<RelayPins> for GpioClearMasks {
    fn from(pins: RelayPins) -> Self {
        let mut masks = Self {
            gpio_low: 0,
            gpio_high: 0,
        };
        for pin in [pins.listen, pins.talk, pins.door] {
            if pin < 32 {
                masks.gpio_low |= 1 << pin;
            } else {
                masks.gpio_high |= 1 << (pin - 32);
            }
        }
        masks
    }
}

/// S3 platform driver owning Core 1 resources until the audio worker is started.
pub struct S3Platform {
    #[cfg(target_os = "none")]
    cpu_control: Option<esp_hal::peripherals::CPU_CTRL<'static>>,
    #[cfg(target_os = "none")]
    core1_interrupt: Option<esp_hal::peripherals::FROM_CPU_INTR1<'static>>,
}

#[cfg(target_os = "none")]
#[esp_hal::ram(unstable(dcache_reclaimed))]
static APP_CORE_STACK: ReclaimedCell<esp_hal::system::Stack<32_768>> = ReclaimedCell::new();

#[cfg(target_os = "none")]
static CORE1_EXECUTOR: static_cell::StaticCell<esp_rtos::embassy::Executor> =
    static_cell::StaticCell::new();

#[cfg(target_os = "none")]
#[embassy_executor::task]
async fn sample_audio_stack_watermark() {
    loop {
        let bottom = AUDIO_STACK_BOTTOM.load(portable_atomic::Ordering::Acquire) as *const u8;
        let untouched = (0..AUDIO_STACK_BYTES)
            .take_while(|offset| unsafe { core::ptr::read_volatile(bottom.add(*offset)) } == STACK_WATERMARK)
            .count();
        AUDIO_STACK_LOW_WATER.fetch_min(untouched, portable_atomic::Ordering::Relaxed);
        embassy_time::Timer::after_millis(250).await;
    }
}

/// Minimum untouched bytes observed at the bottom of the Core 1 audio stack.
#[cfg(target_os = "none")]
pub fn audio_stack_free_low_water_bytes() -> usize {
    AUDIO_STACK_LOW_WATER.load(portable_atomic::Ordering::Relaxed)
}

#[cfg(target_os = "none")]
pub fn allocate_webrtc_jitter_buffer(
) -> &'static mut intercom_adapter_webrtc::session::WebRtcJitterBuffer {
    match alloc_psram_in_place::<intercom_adapter_webrtc::session::WebRtcJitterBuffer>(
        PsramInitialization::InPlace(|storage| {
            storage.write(intercom_adapter_webrtc::session::WebRtcJitterBuffer::new(0))
        }),
    ) {
        PsramAllocation::Initialized(value) => value,
        PsramAllocation::Uninitialized(_) => unreachable!(),
    }
}

/// Allocate uninitialized storage for one value in S3 PSRAM.
#[cfg(target_os = "none")]
pub fn alloc_psram_uninit<T: 'static>() -> &'static mut core::mem::MaybeUninit<T> {
    match alloc_psram_in_place::<T>(PsramInitialization::Uninitialized) {
        PsramAllocation::Uninitialized(storage) => storage,
        PsramAllocation::Initialized(_) => unreachable!(),
    }
}

/// Types whose valid initialized representation is all zero bytes.
#[cfg(target_os = "none")]
trait PsramZeroValid {}

#[cfg(target_os = "none")]
impl PsramZeroValid for intercom_adapter_webrtc::dtls::DtlsSrtpResources {}

#[cfg(target_os = "none")]
enum PsramInitialization<T> {
    Uninitialized,
    Zeroed,
    InPlace(for<'a> fn(&'a mut core::mem::MaybeUninit<T>) -> &'a mut T),
}

#[cfg(target_os = "none")]
enum PsramAllocation<T: 'static> {
    Uninitialized(&'static mut core::mem::MaybeUninit<T>),
    Initialized(&'static mut T),
}

#[cfg(target_os = "none")]
fn zeroed_psram_initialization<T: PsramZeroValid>() -> PsramInitialization<T> {
    PsramInitialization::Zeroed
}

/// Allocate and initialize one S3 PSRAM value in place.
///
/// The `Zeroed` initializer is constructed only for `DtlsSrtpResources`, whose fields
/// are integer arrays and accept an all-zero representation. Allocation alignment and
/// size come from `Layout::new::<T>()`; the allocator owns the returned region forever.
#[cfg(target_os = "none")]
fn alloc_psram_in_place<T: 'static>(initialize: PsramInitialization<T>) -> PsramAllocation<T> {
    let layout = core::alloc::Layout::new::<T>();
    log::info!(
        "S3 PSRAM allocation requested: size={} align={} free={}",
        layout.size(),
        layout.align(),
        esp_alloc::HEAP.free_caps(esp_alloc::MemoryCapability::External.into())
    );
    let allocation =
        unsafe { esp_alloc::HEAP.alloc_caps(esp_alloc::MemoryCapability::External.into(), layout) };
    assert!(!allocation.is_null(), "S3 PSRAM allocation failed");
    log::info!(
        "S3 PSRAM allocation complete: size={} free={}",
        layout.size(),
        esp_alloc::HEAP.free_caps(esp_alloc::MemoryCapability::External.into())
    );
    // SAFETY: `allocation` is non-null, aligned to `layout.align()`, and sized for `T`.
    let storage = unsafe { &mut *allocation.cast::<core::mem::MaybeUninit<T>>() };
    match initialize {
        PsramInitialization::Uninitialized => PsramAllocation::Uninitialized(storage),
        PsramInitialization::Zeroed => {
            // SAFETY: This variant is only built through `zeroed_psram_initialization`,
            // whose bound restricts it to the audited integer-array DTLS resource type.
            unsafe {
                core::ptr::write_bytes(storage.as_mut_ptr().cast::<u8>(), 0, layout.size());
                PsramAllocation::Initialized(storage.assume_init_mut())
            }
        }
        PsramInitialization::InPlace(initialize) => {
            PsramAllocation::Initialized(initialize(storage))
        }
    }
}

/// Allocate and zero the fixed integer-array DTLS storage directly in PSRAM.
#[cfg(target_os = "none")]
pub fn allocate_dtls_srtp_resources(
) -> &'static mut intercom_adapter_webrtc::dtls::DtlsSrtpResources {
    match alloc_psram_in_place(zeroed_psram_initialization()) {
        PsramAllocation::Initialized(resources) => resources,
        PsramAllocation::Uninitialized(_) => unreachable!(),
    }
}

#[cfg(target_os = "none")]
pub fn allocate_webrtc_media_resources(
) -> &'static mut intercom_adapter_webrtc::session::WebRtcMediaResources {
    match alloc_psram_in_place(PsramInitialization::InPlace(
        intercom_adapter_webrtc::session::WebRtcMediaResources::init_in,
    )) {
        PsramAllocation::Initialized(resources) => resources,
        PsramAllocation::Uninitialized(_) => unreachable!(),
    }
}

#[cfg(target_os = "none")]
impl PlatformDriver for S3Platform {
    type Drivers = PlatformDrivers<PlatformPeripherals>;

    fn init() -> (Self, Self::Drivers) {
        let peripherals = esp_hal::init(
            esp_hal::Config::default().with_cpu_clock(esp_hal::clock::CpuClock::max()),
        );
        let mut startup_seed = [0; 32];
        {
            let entropy_source = esp_hal::rng::TrngSource::new(peripherals.RNG, peripherals.ADC1);
            let trng = esp_hal::rng::Trng::try_new()
                .expect("S3 SAR ADC entropy source must seed startup randomness");
            trng.read(&mut startup_seed);
            let _ = trng.downgrade();
            drop(entropy_source);
        }
        super::entropy::initialize_global(startup_seed);
        super::entropy::set_radio_entropy_source(fill_radio_entropy);

        let psram = esp_hal::psram::Psram::new(
            peripherals.PSRAM,
            esp_hal::psram::PsramConfig {
                mode: esp_hal::psram::PsramMode::OctalSpi,
                size: esp_hal::psram::PsramSize::Size(8 * 1024 * 1024),
                ..Default::default()
            },
        );
        let (psram_start, psram_size) = psram.raw_parts();
        assert_eq!(
            psram_size,
            8 * 1024 * 1024,
            "ESP32-S3-N16R8 requires 8 MiB Octal SPI PSRAM"
        );
        S3MemoryProfile::initialize(psram_size)
            .expect("ESP32-S3 profile requires initialized PSRAM");
        unsafe {
            esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
                psram_start,
                psram_size,
                esp_alloc::MemoryCapability::External.into(),
            ));
        }

        let timg0 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
        esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);
        let cpu_control = peripherals.CPU_CTRL;
        let core1_interrupt = peripherals.FROM_CPU_INTR1;

        let esp_hal::peripherals::Peripherals {
            WIFI,
            BT,
            I2C0,
            GPIO0,
            GPIO1,
            GPIO2,
            GPIO4,
            GPIO7,
            GPIO8,
            GPIO9,
            GPIO10,
            GPIO11,
            GPIO12,
            GPIO13,
            GPIO14,
            GPIO15,
            GPIO16,
            GPIO17,
            GPIO18,
            GPIO19,
            GPIO20,
            GPIO21,
            GPIO40,
            GPIO47,
            GPIO48,
            I2S0,
            DMA_CH0,
            RMT,
            ..
        } = peripherals;

        let drivers = PlatformDrivers {
            peripherals: PlatformPeripherals {
                WIFI,
                BT,
                I2C0,
                GPIO0,
                GPIO1,
                GPIO2,
                GPIO7,
                GPIO8,
                GPIO9,
                GPIO10,
                GPIO11,
                GPIO15,
                GPIO18,
                GPIO19,
                GPIO20,
                GPIO21,
                I2S0,
                DMA_CH0,
                GPIO4,
                GPIO12,
                GPIO13,
                GPIO14,
                GPIO16,
                GPIO17,
                GPIO40,
                GPIO47,
                GPIO48,
                RMT,
            },
        };

        (
            Self {
                cpu_control: Some(cpu_control),
                core1_interrupt: Some(core1_interrupt),
            },
            drivers,
        )
    }

    fn spawn_audio_worker(
        &mut self,
        _spawner: embassy_executor::Spawner,
        worker: impl FnOnce(embassy_executor::Spawner) + Send + 'static,
    ) {
        let cpu_control = self
            .cpu_control
            .take()
            .expect("Core 1 can only be started once");
        let core1_interrupt = self
            .core1_interrupt
            .take()
            .expect("Core 1 interrupt can only be consumed once");
        let stack = unsafe {
            let stack = APP_CORE_STACK.init(esp_hal::system::Stack::new());
            AUDIO_STACK_BOTTOM.store(stack.bottom() as usize, portable_atomic::Ordering::Release);
            core::ptr::write_bytes(
                stack.bottom().cast::<u8>(),
                STACK_WATERMARK,
                AUDIO_STACK_BYTES,
            );
            stack
        };
        esp_rtos::start_second_core(cpu_control, core1_interrupt, stack, move || {
            let executor = CORE1_EXECUTOR.init(esp_rtos::embassy::Executor::new());
            executor.run(|core1_spawner| {
                core1_spawner
                    .spawn(sample_audio_stack_watermark().expect("Failed to create stack sampler"));
                worker(core1_spawner);
            });
        });
    }
}

/// Memory capability requested for a subsystem allocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryPlacement {
    Internal,
    External,
}

/// Failure to initialize the required S3 PSRAM profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryProfileError {
    PsramUnavailable,
}

/// Return the portion of the internal heap that can use bootloader-reclaimed RAM.
pub const fn reclaimed_heap_size(
    total_heap_bytes: usize,
    reclaimed_region_bytes: usize,
    reserved_bytes: usize,
) -> usize {
    if reclaimed_region_bytes <= reserved_bytes {
        return 0;
    }
    let available = reclaimed_region_bytes - reserved_bytes;
    if total_heap_bytes < available {
        total_heap_bytes
    } else {
        available
    }
}

/// Internal heap slices routed to the three linker regions that are usable by the S3 profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InternalHeapLayout {
    pub standard_bytes: usize,
    pub boot_reclaimed_bytes: usize,
    pub dcache_reclaimed_bytes: usize,
}

/// Fit the configured heap around DTLS state and both reclaimed internal RAM regions.
pub const fn heap_region_sizes(
    total_heap_bytes: usize,
    boot_reclaimed_region_bytes: usize,
    dcache_reclaimed_region_bytes: usize,
    reserved_boot_reclaimed_bytes: usize,
    reserved_dcache_reclaimed_bytes: usize,
) -> InternalHeapLayout {
    let boot_reclaimed_bytes = reclaimed_heap_size(
        total_heap_bytes,
        boot_reclaimed_region_bytes,
        reserved_boot_reclaimed_bytes,
    );
    let after_boot_region = total_heap_bytes - boot_reclaimed_bytes;
    let dcache_available = if dcache_reclaimed_region_bytes > reserved_dcache_reclaimed_bytes {
        dcache_reclaimed_region_bytes - reserved_dcache_reclaimed_bytes
    } else {
        0
    };
    let dcache_reclaimed_bytes = if after_boot_region < dcache_available {
        after_boot_region
    } else {
        dcache_available
    };

    InternalHeapLayout {
        standard_bytes: after_boot_region - dcache_reclaimed_bytes,
        boot_reclaimed_bytes,
        dcache_reclaimed_bytes,
    }
}

#[cfg(any(target_os = "none", test))]
pub const fn heap_region_is_present(bytes: usize) -> bool {
    bytes != 0
}

/// S3 data-cache reclaimable RAM when `ESP_HAL_CONFIG_DATA_CACHE_SIZE=32KB`.
pub const DCACHE_RECLAIMED_BYTES: usize = 32 * 1024;

/// S3 internal heap budget selected to preserve the linker-managed main stack.
pub const INTERNAL_HEAP_BYTES: usize = 120 * 1024;

/// Build the S3 internal heap layout while keeping the shared configured budget.
pub const fn internal_heap_layout(
    total_heap_bytes: usize,
    boot_reclaimed_region_bytes: usize,
    dcache_reclaimed_region_bytes: usize,
    reserved_boot_reclaimed_bytes: usize,
    reserved_dcache_reclaimed_bytes: usize,
) -> InternalHeapLayout {
    heap_region_sizes(
        total_heap_bytes,
        boot_reclaimed_region_bytes,
        dcache_reclaimed_region_bytes,
        reserved_boot_reclaimed_bytes,
        reserved_dcache_reclaimed_bytes,
    )
}

/// Allocation policy for an S3 board with external PSRAM.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct S3MemoryProfile {
    pub media_resources: MemoryPlacement,
    pub jitter_buffer: MemoryPlacement,
    pub audio_worker_stack: MemoryPlacement,
    pub dma_buffers: MemoryPlacement,
}

impl S3MemoryProfile {
    /// Validate PSRAM initialization and return the required allocation policy.
    pub const fn initialize(psram_bytes: usize) -> Result<Self, MemoryProfileError> {
        if psram_bytes == 0 {
            return Err(MemoryProfileError::PsramUnavailable);
        }
        Ok(Self {
            media_resources: MemoryPlacement::External,
            jitter_buffer: MemoryPlacement::External,
            audio_worker_stack: MemoryPlacement::Internal,
            dma_buffers: MemoryPlacement::Internal,
        })
    }
}

#[cfg(all(test, feature = "esp32s3"))]
mod tests {
    use super::{
        heap_region_is_present, heap_region_sizes, internal_heap_layout, reclaimed_heap_size,
        stack_watermark_bytes, BoardPins, GpioClearMasks, MemoryPlacement, MemoryProfileError,
        RelayPins, S3MemoryProfile, EXPOSED_PINS, RESERVED_PINS,
    };

    #[test]
    fn validates_pin_roles_and_relay_clear_masks() {
        // Arrange
        let board = BoardPins::SUPPORTED;
        let pins = board.all_signal_pins();

        // Act
        let are_unique = pins
            .iter()
            .enumerate()
            .all(|(index, pin)| !pins[..index].contains(pin));
        let are_exposed = pins.iter().all(|pin| EXPOSED_PINS.contains(pin));
        let avoid_reserved = pins.iter().all(|pin| !RESERVED_PINS.contains(pin));
        let relay_masks = GpioClearMasks::from(RelayPins {
            listen: board.bus_listen,
            talk: board.bus_talk,
            door: board.bus_door,
        });

        // Assert
        assert!(are_unique, "board signal pins must not overlap");
        assert!(are_exposed, "every board signal pin must be exposed");
        assert!(
            avoid_reserved,
            "board signals must avoid flash, PSRAM, and console pins"
        );
        assert_eq!(relay_masks.gpio_low, (1 << 17) | (1 << 18));
        assert_eq!(relay_masks.gpio_high, 1 << (40 - 32));
        assert!(RESERVED_PINS.contains(&38));
    }

    #[test]
    fn requires_psram_and_keeps_dma_and_stack_internal() {
        // Arrange
        let no_psram = S3MemoryProfile::initialize(0);

        // Act
        let profile = S3MemoryProfile::initialize(8 * 1024 * 1024);

        // Assert
        assert_eq!(no_psram, Err(MemoryProfileError::PsramUnavailable));
        let profile = profile.unwrap();
        assert_eq!(profile.jitter_buffer, MemoryPlacement::External);
        assert_eq!(profile.media_resources, MemoryPlacement::External);
        assert_eq!(profile.audio_worker_stack, MemoryPlacement::Internal);
        assert_eq!(profile.dma_buffers, MemoryPlacement::Internal);
    }

    #[test]
    fn devkit_rgb_led_uses_gpio47_rmt_driver() {
        assert!(cfg!(feature = "led-smartled"));
        assert_eq!(BoardPins::SUPPORTED.indicator_led, 47);
    }

    #[test]
    fn reclaims_bootloader_ram_without_changing_heap_budget() {
        let total_heap = 120 * 1024;
        let reclaimed_region = 73_744;
        let dtls_reservation = 32 * 1024;

        let reclaimed_heap = reclaimed_heap_size(total_heap, reclaimed_region, dtls_reservation);

        assert_eq!(reclaimed_heap, reclaimed_region - dtls_reservation);
        assert_eq!(total_heap - reclaimed_heap, 120 * 1024 - reclaimed_heap);
        assert_eq!(reclaimed_heap + (total_heap - reclaimed_heap), total_heap);
    }

    #[test]
    fn distributes_heap_across_boot_and_dcache_reclaimed_ram() {
        let layout = heap_region_sizes(120 * 1024, 73_744, 32 * 1024, 32 * 1024, 0);

        assert_eq!(layout.boot_reclaimed_bytes, 73_744 - 32 * 1024);
        assert_eq!(layout.dcache_reclaimed_bytes, 32 * 1024);
        assert_eq!(layout.standard_bytes, 120 * 1024 - 73_744);
        assert_eq!(
            layout.boot_reclaimed_bytes + layout.dcache_reclaimed_bytes + layout.standard_bytes,
            120 * 1024
        );
    }

    #[test]
    fn s3_release_heap_preserves_the_linker_main_stack_reserve() {
        let layout = internal_heap_layout(120 * 1024, 73_744, 32 * 1024, 0, 32 * 1024);

        assert_eq!(
            layout.standard_bytes + layout.boot_reclaimed_bytes + layout.dcache_reclaimed_bytes,
            crate::config::HEAP_SIZE
        );
        assert_eq!(crate::config::HEAP_SIZE, 120 * 1024);
        assert_eq!(layout.boot_reclaimed_bytes, 73_744);
        assert_eq!(layout.dcache_reclaimed_bytes, 0);
        assert_eq!(layout.standard_bytes, 49_136);
    }

    #[test]
    fn empty_heap_regions_are_not_registered() {
        assert!(!heap_region_is_present(0));
        assert!(heap_region_is_present(1));
    }

    #[test]
    fn reserves_dcache_reclaimed_ram_for_the_audio_worker_stack() {
        let layout = heap_region_sizes(120 * 1024, 73_744, 32 * 1024, 0, 32 * 1024);

        assert_eq!(layout.boot_reclaimed_bytes, 73_744);
        assert_eq!(layout.dcache_reclaimed_bytes, 0);
        assert_eq!(layout.standard_bytes, 49_136);
    }

    #[test]
    fn stack_watermark_counts_only_the_untouched_low_address_prefix() {
        // Arrange
        const STACK_WATERMARK: u8 = 0xA5;
        let mut stack = [STACK_WATERMARK; 16];
        stack[12..].fill(0x00);

        // Act
        let unused = stack_watermark_bytes(&stack);

        // Assert
        assert_eq!(unused, 12);
    }
}
