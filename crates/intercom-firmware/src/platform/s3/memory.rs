//! ESP32-S3 internal heap budget calculations and PSRAM allocators.

/// S3 data-cache reclaimable RAM when `ESP_HAL_CONFIG_DATA_CACHE_SIZE=32KB`.
pub const DCACHE_RECLAIMED_BYTES: usize = 32 * 1024;

/// S3 internal heap budget selected to preserve the linker-managed main stack.
pub const INTERNAL_HEAP_BYTES: usize = 120 * 1024;

/// Internal heap slices routed to the three linker regions that are usable by the S3 profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InternalHeapLayout {
    pub standard_bytes: usize,
    pub boot_reclaimed_bytes: usize,
    pub dcache_reclaimed_bytes: usize,
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

pub const fn heap_region_is_present(bytes: usize) -> bool {
    bytes != 0
}

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

#[cfg(target_os = "none")]
mod psram {
    /// Allocate uninitialized storage for one value in S3 PSRAM.
    pub fn alloc_psram_uninit<T: 'static>() -> &'static mut core::mem::MaybeUninit<T> {
        match alloc_psram_in_place::<T>(PsramInitialization::Uninitialized) {
            PsramAllocation::Uninitialized(storage) => storage,
            PsramAllocation::Initialized(_) => unreachable!(),
        }
    }

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

    /// Allocate and zero the fixed integer-array DTLS storage directly in PSRAM.
    pub fn allocate_dtls_srtp_resources(
    ) -> &'static mut intercom_adapter_webrtc::dtls::DtlsSrtpResources {
        match alloc_psram_in_place(zeroed_psram_initialization()) {
            PsramAllocation::Initialized(resources) => resources,
            PsramAllocation::Uninitialized(_) => unreachable!(),
        }
    }

    pub fn allocate_webrtc_media_resources(
    ) -> &'static mut intercom_adapter_webrtc::session::WebRtcMediaResources {
        match alloc_psram_in_place(PsramInitialization::InPlace(
            intercom_adapter_webrtc::session::WebRtcMediaResources::init_in,
        )) {
            PsramAllocation::Initialized(resources) => resources,
            PsramAllocation::Uninitialized(_) => unreachable!(),
        }
    }

    /// Types whose valid initialized representation is all zero bytes.
    trait PsramZeroValid {}

    impl PsramZeroValid for intercom_adapter_webrtc::dtls::DtlsSrtpResources {}

    enum PsramInitialization<T> {
        Uninitialized,
        Zeroed,
        InPlace(for<'a> fn(&'a mut core::mem::MaybeUninit<T>) -> &'a mut T),
    }

    enum PsramAllocation<T: 'static> {
        Uninitialized(&'static mut core::mem::MaybeUninit<T>),
        Initialized(&'static mut T),
    }

    fn zeroed_psram_initialization<T: PsramZeroValid>() -> PsramInitialization<T> {
        PsramInitialization::Zeroed
    }

    /// Allocate and initialize one S3 PSRAM value in place.
    fn alloc_psram_in_place<T: 'static>(initialize: PsramInitialization<T>) -> PsramAllocation<T> {
        let layout = core::alloc::Layout::new::<T>();
        log::info!(
            "S3 PSRAM allocation requested: size={} align={} free={}",
            layout.size(),
            layout.align(),
            esp_alloc::HEAP.free_caps(esp_alloc::MemoryCapability::External.into())
        );
        let allocation = unsafe {
            esp_alloc::HEAP.alloc_caps(esp_alloc::MemoryCapability::External.into(), layout)
        };
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
}

#[cfg(target_os = "none")]
pub use psram::{
    alloc_psram_uninit, allocate_dtls_srtp_resources, allocate_webrtc_jitter_buffer,
    allocate_webrtc_media_resources,
};
