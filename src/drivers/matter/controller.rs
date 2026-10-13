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

use core::pin::pin;

use embassy_futures::select::select;
use embassy_time::{Duration, Timer};
use log::{info, warn};

use rs_matter::dm::clusters::basic_info::BasicInfoConfig;
use rs_matter::dm::clusters::desc::{self, ClusterHandler as _};
use rs_matter::dm::{
    Async, AsyncHandler, ChainedHandler, Cluster, Dataver, DeviceType, EmptyHandler, Endpoint,
    Node,
};
use rs_matter::persist::KvBlobStore;
use rs_matter::BasicCommData;

use rs_matter_embassy::matter::crypto::{default_crypto, Crypto};
use rs_matter_embassy::matter::dm::devices::test::{DAC_PRIVKEY, TEST_DEV_ATT};
use rs_matter_embassy::matter::utils::init::InitMaybeUninit;
use rs_matter_embassy::stack::rand::reseeding_csprng;
use rs_matter_embassy::stack::UserTask;
use rs_matter_embassy::wireless::esp::EspWifiDriver;
use rs_matter_embassy::wireless::{EmbassyWifi, EmbassyWifiMatterStack};

use super::{commissioning, MatterCluster, BUMP_SIZE};

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
///
/// Generic over `S: KvBlobStore` — the persistence backend for Matter
/// commissioning data. Typically `EmbassyKvBlobStore<EspFlash>` on real
/// hardware, but can be swapped for testing or alternative flash drivers.
pub struct MatterControllerConfig<S: KvBlobStore> {
    /// Hardware peripherals for WiFi+BLE.
    pub peripherals: MatterPeripherals,
    /// Device identity for the Basic Information cluster.
    pub device_info: DeviceInfo,
    /// KV blob store for Matter commissioning data persistence.
    pub store: S,
    /// Optional commissioning data override (if None, derived from hardware MAC).
    pub comm_data: Option<BasicCommData>,
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

pub struct MatterController<S: KvBlobStore, OnConnecting, OnConnected, H = EmptyHandler>
where
    OnConnecting: Fn(),
    OnConnected: Fn(),
{
    handler: H,
    endpoint_metas: Vec<EndpointMeta>,
    config: MatterControllerConfig<S>,
    on_connecting: OnConnecting,
    on_connected: OnConnected,
}

impl<S: KvBlobStore, OnConnecting: Fn(), OnConnected: Fn()>
    MatterController<S, OnConnecting, OnConnected>
{
    /// Start building a new Matter WiFi node with an empty handler chain.
    ///
    /// `on_connecting` is called when the WiFi/CASE handshake starts.
    /// `on_connected` is called each time the operational network comes up.
    pub fn new(
        config: MatterControllerConfig<S>,
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

impl<S: KvBlobStore, OnConnecting: Fn(), OnConnected: Fn(), H: AsyncHandler>
    MatterController<S, OnConnecting, OnConnected, H>
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
    ) -> MatterController<S, OnConnecting, OnConnected, impl AsyncHandler + 'b>
    where
        H: 'b,
        S: 'b,
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

        let new_handler = ChainedHandler::new_with_matcher(
            move |e, c| e == endpoint_id && c == desc::DescHandler::CLUSTER.id,
            Async(desc::DescHandler::new(Dataver::new(dataver_seed)).adapt()),
            ChainedHandler::new_with_matcher(
                move |e, c| e == endpoint_id && c == C::CLUSTER_DEF.id,
                cluster,
                self.handler,
            ),
        );

        MatterController::<S, _, _, _> {
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
    /// crypto, persistence, WiFi driver, and the main event loop.
    ///
    /// # Panics
    /// Panics if the Matter stack encounters a fatal error.
    pub async fn run(self) {
        let config = self.config;
        // Build the Node descriptor from registered endpoints.
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
            endpoints.push(Endpoint::new(
                meta.id,
                &dtype_arrays[i],
                &cluster_arrays[i],
            ));
        }

        let node = Node {
            endpoints: &endpoints,
        };

        let comm_data = if let Some(custom_comm) = config.comm_data {
            info!(
                "Using custom commissioning parameters (discriminator: {})",
                custom_comm.discriminator
            );
            custom_comm
        } else {
            let mac = esp_hal::efuse::base_mac_address();
            let mac_bytes: [u8; 6] = mac.as_bytes().try_into().unwrap();
            let data = commissioning::comm_data_from_mac(&mac_bytes);
            let passcode = commissioning::passcode_from_mac(&mac_bytes);
            info!(
                "Generated commissioning parameters from MAC {}: passcode {}, discriminator {}",
                mac, passcode, data.discriminator
            );
            data
        };

        // Allocate device identity + stack in static cells (safe: run() is called exactly once)
        static DEV_DET_CELL: static_cell::StaticCell<BasicInfoConfig> =
            static_cell::StaticCell::new();
        let dev_det = DEV_DET_CELL.init(dev_det(&config.device_info));

        static STACK_CELL: static_cell::StaticCell<EmbassyWifiMatterStack<BUMP_SIZE, ()>> =
            static_cell::StaticCell::new();
        let stack = STACK_CELL.uninit().init_with(EmbassyWifiMatterStack::init(
            dev_det,
            comm_data,
            &TEST_DEV_ATT,
        ));

        // Crypto: TRNG → reseeding CSPRNG → DAC signing key
        let _trng_source =
            esp_hal::rng::TrngSource::new(config.peripherals.rng, config.peripherals.adc1);
        let crypto = default_crypto(
            reseeding_csprng(esp_hal::rng::Trng::try_new().unwrap(), 1000).unwrap(),
            DAC_PRIVKEY,
        );
        let weak_rand = crypto.weak_rand().unwrap();

        // Chain root endpoint (Endpoint 0) system clusters (Descriptor, Basic Info, Admin Comm, NOC, ACL, etc.)
        let handler = ChainedHandler::new_with_matcher(
            |e, _| e == rs_matter::dm::endpoints::ROOT_ENDPOINT_ID,
            Async(EmbassyWifiMatterStack::<0, ()>::root_handler(
                &(),
                crypto.weak_rand().unwrap(),
            )),
            self.handler,
        );

        // Load persisted state + open commissioning window if not yet commissioned.
        // If loading fails (e.g. TLV format mismatch after firmware upgrade),
        // reset the KV store and retry so the device doesn't boot-loop.
        let mut kv = config.store;
        if let Err(e) = stack.startup(&crypto, &mut kv).await {
            warn!("Failed to load persisted state ({:?}), resetting KV store", e);
            stack
                .reset(&crypto, (&node, &handler), &mut kv)
                .await
                .unwrap();
            stack.startup(&crypto, &mut kv).await.unwrap();
        }

        // Wrap KV store as shared reference for concurrent access by Matter stack
        let kv = stack.matter().kv(kv);

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
            matter: stack.matter(),
        };
        stack
            .run_coex(
                EmbassyWifi::new(
                    EspWifiDriver::new(config.peripherals.wifi, config.peripherals.bt),
                    weak_rand,
                    true,
                    stack,
                ),
                &crypto,
                (node, handler),
                kv,
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
/// Also monitors for decommissioning (all fabrics removed from Apple Home)
/// and reboots the device to re-enter commissioning mode.
///
/// Implements `UserTask`: runs alongside the Matter stack from startup.
/// Uses `NetifDiag` to check if any interface is operational, and
/// `NetChangeNotif` to wait for state changes. Signals `on_connected`
/// when WiFi becomes operational and `on_connecting` when it drops.
struct ConnectionMonitor<'a, OnConnecting: Fn(), OnConnected: Fn()> {
    on_connecting: OnConnecting,
    on_connected: OnConnected,
    matter: &'a rs_matter::Matter<'a>,
}

impl<OnConnecting: Fn(), OnConnected: Fn()> UserTask
    for ConnectionMonitor<'_, OnConnecting, OnConnected>
{
    async fn run<S, N>(&mut self, _net_stack: S, netif: N) -> Result<(), rs_matter::error::Error>
    where
        S: rs_matter_embassy::stack::nal::NetStack,
        N: rs_matter::dm::clusters::gen_diag::NetifDiag + rs_matter::dm::networks::NetChangeNotif,
    {
        let mut was_operational = false;
        let mut was_commissioned = self.matter.has_fabrics();

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

            // Wait for a network state change OR a periodic tick.
            // The tick lets us detect decommissioning (all fabrics removed)
            // without stealing the persist notification from MatterPersist
            // (embassy Notification supports only one waiter).
            let net_changed = pin!(netif.wait_changed());
            let tick = pin!(Timer::after(Duration::from_secs(5)));

            select(net_changed, tick).await;

            // Detect transition from commissioned → uncommissioned
            // (all fabrics removed via RemoveFabric, i.e. device removed
            // from Apple Home). By the time the timer fires the persist task
            // has already saved the empty state to flash, so the reboot will
            // correctly enter commissioning mode.
            let is_commissioned = self.matter.has_fabrics();
            if was_commissioned && !is_commissioned {
                warn!("All fabrics removed — device decommissioned, rebooting to re-enter commissioning mode");
                esp_hal::system::software_reset();
            }
            was_commissioned = is_commissioned;
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
