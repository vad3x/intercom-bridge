//! Matter-over-WiFi node driver.
//!
//! `MatterController` is a builder+runner for the full rs-matter-embassy WiFi
//! stack in **concurrent** (coex) commissioning mode. BLE and WiFi run
//! simultaneously, sharing the ESP32-C6's single radio via the coexistence
//! driver. Cluster handlers are registered via `.register()` and the stack
//! is launched with `.run()`.
//!
//! ```ignore
//! let matter = MatterController::new(MatterControllerConfig { /* ... */ })
//!     .register(&door_lock)
//!     .register(&occupancy);
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
use rs_matter_embassy::persist::EmbassyKvBlobStore;
use rs_matter_embassy::stack::rand::reseeding_csprng;
use rs_matter_embassy::stack::UserTask;
use rs_matter_embassy::wireless::esp::EspWifiDriver;
use rs_matter_embassy::wireless::{EmbassyWifi, EmbassyWifiMatterStack};

use crate::drivers::flash::EspFlash;

use super::{MatterCluster, BUMP_SIZE};

// ============================================================================
// Device identity — replaces rs-matter TEST_DEV_* defaults
// ============================================================================

/// Vendor ID (test/development — 0xFFF1)
const VID: u16 = 0xfff1;
/// Product ID (test/development — 0x8001)
const PID: u16 = 0x8001;

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the Matter controller.
pub struct MatterControllerConfig {
    /// Hardware peripherals for WiFi+BLE.
    pub peripherals: MatterPeripherals,
    /// Device identity for the Basic Information cluster.
    pub device_info: DeviceInfo,
    /// KV blob store for Matter commissioning data persistence.
    pub persist: EmbassyKvBlobStore<EspFlash>,
}

/// Hardware peripherals required by the WiFi+BLE Matter stack.
pub struct MatterPeripherals {
    pub wifi: esp_hal::peripherals::WIFI<'static>,
    pub bt: esp_hal::peripherals::BT<'static>,
    pub rng: esp_hal::peripherals::RNG<'static>,
    pub adc1: esp_hal::peripherals::ADC1<'static>,
}

/// Device identity exposed via the Matter Basic Information cluster.
pub struct DeviceInfo {
    /// Hardware version number.
    pub hw_ver: u16,
    /// Hardware version string (e.g. `"1.0"`).
    pub hw_ver_str: &'static str,
    /// Numeric software version (e.g. `100` for `0.1.0`).
    pub sw_ver: u32,
    /// Human-readable software version string (e.g. `"0.1.0"`).
    pub sw_ver_str: &'static str,
    /// Device serial number.
    pub serial_no: &'static str,
    /// Vendor name shown in Matter/HomeKit.
    pub vendor_name: &'static str,
    /// Product name shown in Matter/HomeKit.
    pub product_name: &'static str,
    /// Device name shown in Matter/HomeKit.
    pub device_name: &'static str,
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

pub struct MatterController<OnConnecting, OnConnected, H = EmptyHandler>
where
    OnConnecting: Fn(),
    OnConnected: Fn(),
{
    handler: H,
    endpoint_metas: Vec<EndpointMeta>,
    config: MatterControllerConfig,
    on_connecting: OnConnecting,
    on_connected: OnConnected,
}

impl<OnConnecting: Fn(), OnConnected: Fn()> MatterController<OnConnecting, OnConnected> {
    /// Start building a new Matter WiFi node with an empty handler chain.
    ///
    /// `on_connecting` is called when the WiFi/CASE handshake starts.
    /// `on_connected` is called each time the operational network comes up.
    pub fn new(
        config: MatterControllerConfig,
        on_connecting: OnConnecting,
        on_connected: OnConnected,
    ) -> Self {
        Self {
            handler: EmptyHandler,
            endpoint_metas: Vec::new(),
            config,
            on_connecting,
            on_connected,
        }
    }
}

impl<OnConnecting: Fn(), OnConnected: Fn(), H: AsyncHandler>
    MatterController<OnConnecting, OnConnected, H>
{
    /// Register a cluster handler.
    ///
    /// The endpoint ID is obtained from `C::ENDPOINT_ID` (the [`MatterCluster`] trait).
    /// Chains the cluster handler **and** a descriptor handler for that
    /// endpoint into the handler chain, and records the endpoint metadata
    /// so `run()` can build the `Node` descriptor automatically.
    pub fn register<'b, C: MatterCluster>(
        mut self,
        cluster: &'b C,
    ) -> MatterController<OnConnecting, OnConnected, impl AsyncHandler + 'b>
    where
        H: 'b,
        OnConnecting: 'b,
        OnConnected: 'b,
    {
        let endpoint_id = C::ENDPOINT_ID;

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
            config: self.config,
            on_connecting: self.on_connecting,
            on_connected: self.on_connected,
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
        let config = self.config;
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

        // Allocate device identity + stack in static cells (safe: run() is called exactly once)
        static DEV_DET_CELL: static_cell::StaticCell<BasicInfoConfig> =
            static_cell::StaticCell::new();
        let dev_det = DEV_DET_CELL.init(dev_det(&config.device_info));

        static STACK_CELL: static_cell::StaticCell<EmbassyWifiMatterStack<BUMP_SIZE, ()>> =
            static_cell::StaticCell::new();
        let stack = STACK_CELL.uninit().init_with(EmbassyWifiMatterStack::init(
            dev_det,
            TEST_DEV_COMM,
            &TEST_DEV_ATT,
            epoch,
        ));

        // Initialize WiFi + BLE radio
        let radio = esp_radio::init().unwrap();

        // Crypto: TRNG → reseeding CSPRNG → DAC signing key
        let _trng_source =
            esp_hal::rng::TrngSource::new(config.peripherals.rng, config.peripherals.adc1);
        let crypto = default_crypto::<NoopRawMutex, _>(
            reseeding_csprng(esp_hal::rng::Trng::try_new().unwrap(), 1000).unwrap(),
            DAC_PRIVKEY,
        );
        let weak_rand = crypto.weak_rand().unwrap();

        // Persistence (commissioning data -> flash partition)
        let persist = stack
            .create_persist_with_comm_window(&crypto, config.persist)
            .await
            .unwrap();

        info!("Matter stack initialized, starting concurrent commissioning (BLE + WiFi coex)");

        // Signal that WiFi/CASE negotiation is starting
        (self.on_connecting)();

        // Run the Matter stack forever.
        // Concurrent mode (`run_coex`): BLE and WiFi run simultaneously
        // via the coexistence driver. Required because sequential mode
        // (BLE then WiFi) breaks WiFi on ESP32-C6 — the radio doesn't
        // properly transition from BLE-only to WiFi-only.
        let mut monitor = ConnectionMonitor {
            on_connecting: self.on_connecting,
            on_connected: self.on_connected,
        };
        stack
            .run_coex(
                EmbassyWifi::new(
                    EspWifiDriver::new(&radio, config.peripherals.wifi, config.peripherals.bt),
                    weak_rand,
                    true,
                    stack,
                ),
                &persist,
                &crypto,
                (node, self.handler),
                &mut monitor,
            )
            .await
            .expect("Matter stack fatal error");
    }
}

// ============================================================================
// ConnectionMonitor — watches netif state and signals connect/disconnect
// ============================================================================

/// Watches the network interface and signals connectivity changes.
///
/// Implements `UserTask`: runs alongside the Matter stack from startup.
/// Uses `NetifDiag` to check if any interface is operational, and
/// `NetChangeNotif` to wait for state changes. Signals `on_connected`
/// when WiFi becomes operational and `on_connecting` when it drops.
struct ConnectionMonitor<OnConnecting: Fn(), OnConnected: Fn()> {
    on_connecting: OnConnecting,
    on_connected: OnConnected,
}

impl<OnConnecting: Fn(), OnConnected: Fn()> UserTask
    for ConnectionMonitor<OnConnecting, OnConnected>
{
    async fn run<S, N>(&mut self, _net_stack: S, netif: N) -> Result<(), rs_matter::error::Error>
    where
        S: rs_matter_embassy::stack::nal::NetStack,
        N: rs_matter::dm::clusters::gen_diag::NetifDiag + rs_matter::dm::networks::NetChangeNotif,
    {
        let mut was_operational = false;

        loop {
            // Check if any network interface is operational
            let mut is_operational = false;
            netif.netifs(&mut |ni| {
                if ni.operational {
                    is_operational = true;
                }
                Ok(())
            })?;

            // Signal state transitions
            if is_operational && !was_operational {
                info!("network up → connected");
                (self.on_connected)();
            } else if !is_operational && was_operational {
                info!("network down → reconnecting");
                (self.on_connecting)();
            }

            was_operational = is_operational;

            // Wait for next network state change
            netif.wait_changed().await;
        }
    }
}

/// Build device identity for the Basic Information cluster.
fn dev_det(info: &DeviceInfo) -> BasicInfoConfig<'static> {
    BasicInfoConfig {
        vid: VID,
        pid: PID,
        hw_ver: info.hw_ver,
        hw_ver_str: info.hw_ver_str,
        sw_ver: info.sw_ver,
        sw_ver_str: info.sw_ver_str,
        serial_no: info.serial_no,
        vendor_name: info.vendor_name,
        product_name: info.product_name,
        device_name: info.device_name,
        ..BasicInfoConfig::new()
    }
}
