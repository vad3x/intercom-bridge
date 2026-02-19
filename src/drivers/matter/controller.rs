//! Matter-over-WiFi node driver.
//!
//! `MatterController` is a builder+runner for the full rs-matter-embassy WiFi
//! coexistence stack. Cluster handlers are registered via `.register()` and
//! the stack is launched with `.run()`.
//!
//! ```ignore
//! let matter = MatterController::new(MatterControllerOptions { /* ... */ })
//!     .register(DOOR_LOCK_ENDPOINT_ID, &door_lock)
//!     .register(OCCUPANCY_SENSOR_ENDPOINT_ID, &occupancy);
//!
//! matter.run().await;
//! ```

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use log::info;

use rs_matter::dm::clusters::basic_info::BasicInfoConfig;
use rs_matter::dm::clusters::desc::{self, ClusterHandler as _};
use rs_matter::dm::{
    Async, AsyncHandler, ChainedHandler, Cluster, Dataver, DeviceType, EmptyHandler, Endpoint,
    EpClMatcher, Node,
};

use rs_matter_embassy::epoch::epoch;
use rs_matter_embassy::matter::crypto::{default_crypto, Crypto};
use rs_matter_embassy::matter::dm::devices::test::{DAC_PRIVKEY, TEST_DEV_ATT, TEST_DEV_COMM};
use rs_matter_embassy::matter::utils::init::InitMaybeUninit;
use rs_matter_embassy::stack::persist::DummyKvBlobStore;
use rs_matter_embassy::stack::rand::reseeding_csprng;
use rs_matter_embassy::wireless::esp::EspWifiDriver;
use rs_matter_embassy::wireless::{EmbassyWifi, EmbassyWifiMatterStack};

use super::{MatterCluster, BUMP_SIZE};

// ============================================================================
// Device identity — replaces rs-matter TEST_DEV_* defaults
// ============================================================================

/// Vendor ID (test/development — 0xFFF1)
const VID: u16 = 0xfff1;
/// Product ID (test/development — 0x8001)
const PID: u16 = 0x8001;

/// Device identity exposed via Basic Information cluster.
const DEV_DET: BasicInfoConfig = BasicInfoConfig {
    vid: VID,
    pid: PID,
    hw_ver: 1,
    hw_ver_str: "1.0",
    sw_ver: 1,
    sw_ver_str: "0.1.0",
    serial_no: "IC-00001",
    vendor_name: "Smart Intercom",
    product_name: "Intercom Controller",
    device_name: "Smart Intercom",
    ..BasicInfoConfig::new()
};

// ============================================================================
// WiFi peripherals config
// ============================================================================

/// Peripherals required by the WiFi+BLE Matter stack.
pub struct MatterControllerOptions {
    pub wifi: esp_hal::peripherals::WIFI<'static>,
    pub bt: esp_hal::peripherals::BT<'static>,
    pub rng: esp_hal::peripherals::RNG<'static>,
    pub adc1: esp_hal::peripherals::ADC1<'static>,
}

// ============================================================================
// Endpoint metadata — collected during register() calls
// ============================================================================

/// Metadata for one registered endpoint, used to build the `Node` descriptor.
struct EndpointMeta {
    id: u16,
    device_type: DeviceType,
    cluster: Cluster<'static>,
}

// ============================================================================
// MatterController — builder + WiFi Matter stack runner
// ============================================================================

/// A dataver counter used to give each descriptor handler a unique seed.
/// Starts above the cluster handler datavers (which use 1, 2, …).
static DESC_DATAVER_COUNTER: portable_atomic::AtomicU32 = portable_atomic::AtomicU32::new(100);

/// Builder+runner for the Matter protocol stack over WiFi.
///
/// Create with [`new()`](Self::new), register cluster handlers with
/// [`register()`](MatterController::register), then launch the stack with
/// [`run()`](MatterController::run).
///
/// ```ignore
/// let matter = MatterController::new(MatterControllerOptions { /* ... */ })
///     .register(DOOR_LOCK_ENDPOINT_ID, &door_lock)
///     .register(OCCUPANCY_SENSOR_ENDPOINT_ID, &occupancy);
///
/// matter.run().await;
/// ```
pub struct MatterController<H = EmptyHandler> {
    handler: H,
    endpoint_metas: Vec<EndpointMeta>,
    options: MatterControllerOptions,
}

impl MatterController {
    /// Start building a new Matter WiFi node with an empty handler chain.
    pub fn new(options: MatterControllerOptions) -> Self {
        Self {
            handler: EmptyHandler,
            endpoint_metas: Vec::new(),
            options,
        }
    }
}

impl<H: AsyncHandler> MatterController<H> {
    /// Register a cluster handler for the given endpoint.
    ///
    /// Chains the cluster handler **and** a descriptor handler for that
    /// endpoint into the handler chain, and records the endpoint metadata
    /// so `run()` can build the `Node` descriptor automatically.
    pub fn register<'a, C: MatterCluster>(
        mut self,
        endpoint_id: u16,
        cluster: &'a C,
    ) -> MatterController<impl AsyncHandler + 'a>
    where
        H: 'a,
    {
        // Record endpoint metadata for the Node descriptor
        self.endpoint_metas.push(EndpointMeta {
            id: endpoint_id,
            device_type: C::DEVICE_TYPE,
            cluster: C::CLUSTER_DEF,
        });

        // Build the handler chain
        let dataver_seed = DESC_DATAVER_COUNTER.fetch_add(1, portable_atomic::Ordering::Relaxed);

        let new_handler = ChainedHandler::new(
            EpClMatcher::new(Some(endpoint_id), Some(desc::DescHandler::CLUSTER.id)),
            Async(desc::DescHandler::new(Dataver::new(dataver_seed)).adapt()),
            ChainedHandler::new(
                EpClMatcher::new(Some(endpoint_id), Some(C::CLUSTER_DEF.id)),
                cluster,
                self.handler,
            ),
        );

        MatterController {
            handler: new_handler,
            endpoint_metas: self.endpoint_metas,
            options: self.options,
        }
    }

    /// Run the Matter protocol stack forever over WiFi.
    ///
    /// Builds the `Node` descriptor from registered endpoints, then
    /// launches the full rs-matter-embassy stack: static stack allocation,
    /// radio init, crypto, persistence, WiFi driver, and the main event loop.
    ///
    /// # Panics
    /// Panics if the Matter stack encounters a fatal error.
    pub async fn run(self) {
        let options = self.options;
        // Build the Node descriptor from registered endpoints.
        // Backing arrays for each endpoint's clusters and device_types.
        let cluster_arrays: Vec<Vec<Cluster>> = self
            .endpoint_metas
            .iter()
            .map(|meta| vec![desc::DescHandler::CLUSTER, meta.cluster.clone()])
            .collect();
        let dtype_arrays: Vec<[DeviceType; 1]> = self
            .endpoint_metas
            .iter()
            .map(|meta| [meta.device_type])
            .collect();

        let mut endpoints: Vec<Endpoint> = Vec::with_capacity(1 + self.endpoint_metas.len());
        endpoints.push(EmbassyWifiMatterStack::<0, ()>::root_endpoint());
        for (i, meta) in self.endpoint_metas.iter().enumerate() {
            endpoints.push(Endpoint {
                id: meta.id,
                device_types: &dtype_arrays[i],
                clusters: &cluster_arrays[i],
            });
        }

        let node = Node {
            id: 0,
            endpoints: &endpoints,
        };

        // Allocate stack in a static cell (safe: run() is called exactly once)
        static STACK_CELL: static_cell::StaticCell<EmbassyWifiMatterStack<BUMP_SIZE, ()>> =
            static_cell::StaticCell::new();
        let stack = STACK_CELL.uninit().init_with(EmbassyWifiMatterStack::init(
            &DEV_DET,
            TEST_DEV_COMM,
            &TEST_DEV_ATT,
            epoch,
        ));

        // Initialize WiFi + BLE radio
        let radio = esp_radio::init().unwrap();

        // Crypto: TRNG → reseeding CSPRNG → DAC signing key
        let _trng_source = esp_hal::rng::TrngSource::new(options.rng, options.adc1);
        let crypto = default_crypto::<NoopRawMutex, _>(
            reseeding_csprng(esp_hal::rng::Trng::try_new().unwrap(), 1000).unwrap(),
            DAC_PRIVKEY,
        );
        let weak_rand = crypto.weak_rand().unwrap();

        // Persistence (commissioning data)
        let persist = stack
            .create_persist_with_comm_window(&crypto, DummyKvBlobStore)
            .await
            .unwrap();

        info!("Matter stack initialized, starting run_coex");

        // Run the Matter stack forever (WiFi coexistence mode)
        stack
            .run_coex(
                EmbassyWifi::new(
                    EspWifiDriver::new(&radio, options.wifi, options.bt),
                    weak_rand,
                    true,
                    stack,
                ),
                &persist,
                &crypto,
                (node, self.handler),
                (),
            )
            .await
            .expect("Matter stack fatal error");
    }
}
