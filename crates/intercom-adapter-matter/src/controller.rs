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

use edf_matter::connection::{has_active_controller_fabric, ConnectionState, FabricPolicy};
use embassy_futures::select::select;
use embassy_time::{Duration, Timer};
use log::{info, warn};

use rs_matter::dm::clusters::basic_info::BasicInfoConfig;
use rs_matter::dm::clusters::desc::{self, ClusterHandler as _};
use rs_matter::dm::clusters::identify;
use rs_matter::dm::{
    Async, AsyncHandler, ChainedHandler, Cluster, Dataver, DeviceType, EmptyHandler, Endpoint, Node,
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

use intercom_core::events::SystemStateSender;
use intercom_core::ports::{ResetScope, SystemResetDriver};

use super::{commissioning, ClusterReadinessBarrier, MatterCluster, BUMP_SIZE};

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

/// Device identity exposed via the Matter Basic Information cluster and DHCPv4.
pub struct DeviceInfo {
    /// Hardware version number.
    pub hw_ver: u16,
    /// Hardware version string (e.g. `"1.0"`).
    pub hw_ver_str: &'static str,
    /// Numeric software version (e.g. `100` for `0.1.0`).
    pub sw_ver: u32,
    /// Human-readable software version string (e.g. `"0.1.0"`).
    pub sw_ver_str: &'static str,
    /// Device serial number override (if None, derived from hardware MAC).
    pub serial_no: Option<&'static str>,
    /// DHCPv4 hostname override (if None, derived from hardware MAC as `"intercom-bridge-xxxx"`).
    pub hostname: Option<&'static str>,
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

pub struct MatterController<S: KvBlobStore, R: SystemResetDriver, H = EmptyHandler> {
    handler: H,
    endpoint_metas: Vec<EndpointMeta>,
    config: MatterControllerConfig<S>,
    state_sender: SystemStateSender<'static>,
    readiness: &'static ClusterReadinessBarrier,
    reset: R,
}

impl<S: KvBlobStore, R: SystemResetDriver> MatterController<S, R> {
    /// Start building a new Matter WiFi node with an empty handler chain.
    ///
    /// Lifecycle state transitions are dispatched to `state_sender`.
    /// Hardware reset/erasure is dispatched to `reset`.
    pub fn new(
        config: MatterControllerConfig<S>,
        state_sender: SystemStateSender<'static>,
        readiness: &'static ClusterReadinessBarrier,
        reset: R,
    ) -> Self {
        Self {
            handler: EmptyHandler,
            endpoint_metas: Vec::new(),
            config,
            state_sender,
            readiness,
            reset,
        }
    }
}

impl<S: KvBlobStore, R: SystemResetDriver, H: AsyncHandler> MatterController<S, R, H> {
    /// Register a cluster handler.
    ///
    /// The endpoint ID is obtained from `C::ENDPOINT_ID` (the [`MatterCluster`] trait).
    /// Chains the cluster handler **and** a descriptor handler for that
    /// endpoint into the handler chain, and records the endpoint metadata
    /// so `run()` can build the `Node` descriptor automatically.
    pub fn register<'b, C: MatterCluster>(
        mut self,
        cluster: &'b C,
    ) -> MatterController<S, R, impl AsyncHandler + 'b>
    where
        H: 'b,
        S: 'b,
        R: 'b,
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
        let id_dataver_seed = DESC_DATAVER_COUNTER.fetch_add(1, portable_atomic::Ordering::Relaxed);

        let new_handler = ChainedHandler::new_with_matcher(
            move |e, c| e == endpoint_id && c == desc::DescHandler::CLUSTER.id,
            Async(desc::DescHandler::new(Dataver::new(dataver_seed)).adapt()),
            ChainedHandler::new_with_matcher(
                move |e, c| e == endpoint_id && c == identify::CLUSTER.id,
                Async(identify::IdentifyHandler::new(Dataver::new(id_dataver_seed)).adapt()),
                ChainedHandler::new_with_matcher(
                    move |e, c| e == endpoint_id && c == C::CLUSTER_DEF.id,
                    cluster,
                    self.handler,
                ),
            ),
        );

        MatterController::<S, R, _> {
            handler: new_handler,
            endpoint_metas: self.endpoint_metas,
            config: self.config,
            state_sender: self.state_sender,
            readiness: self.readiness,
            reset: self.reset,
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
            .map(|meta| {
                vec![
                    desc::DescHandler::CLUSTER,
                    identify::CLUSTER,
                    meta.cluster.clone(),
                ]
            })
            .collect();
        let dtype_arrays: Vec<[DeviceType; 1]> = self
            .endpoint_metas
            .iter()
            .map(|meta| [meta.device_type])
            .collect();

        let mut endpoints: Vec<Endpoint> = Vec::with_capacity(1 + self.endpoint_metas.len());
        endpoints.push(EmbassyWifiMatterStack::<0, ()>::root_endpoint());
        for (i, meta) in self.endpoint_metas.iter().enumerate() {
            endpoints.push(Endpoint::new(meta.id, &dtype_arrays[i], &cluster_arrays[i]));
        }

        let node = Node {
            endpoints: &endpoints,
        };

        let mac = esp_hal::efuse::base_mac_address();
        let mac_bytes: [u8; 6] = mac.as_bytes().try_into().unwrap();

        static IDENTITY_CELL: static_cell::StaticCell<commissioning::DeviceIdentity> =
            static_cell::StaticCell::new();
        let identity = &*IDENTITY_CELL.init(
            commissioning::derive_intercom_identity(&mac_bytes)
                .expect("valid Intercom identity prefixes"),
        );

        let comm_data = if let Some(custom_comm) = config.comm_data {
            info!(
                "Using custom commissioning parameters (discriminator: {})",
                custom_comm.discriminator
            );
            custom_comm
        } else {
            let data = identity.comm_data();
            info!(
                "Generated commissioning parameters from MAC {}: passcode {}, discriminator {}",
                mac, identity.passcode, identity.discriminator
            );
            data
        };

        let serial_no: &'static str = if let Some(custom_serial) = config.device_info.serial_no {
            info!("Using custom serial number: {}", custom_serial);
            custom_serial
        } else {
            let serial_no = identity.serial_str();
            info!("Generated serial number from MAC {}: {}", mac, serial_no);
            serial_no
        };

        let hostname: &'static str = if let Some(custom_hostname) = config.device_info.hostname {
            info!("Using custom DHCP hostname: {}", custom_hostname);
            custom_hostname
        } else {
            let hostname = identity.hostname_str();
            info!("Generated DHCP hostname from MAC {}: {}", mac, hostname);
            hostname
        };

        // Allocate device identity + stack in static cells (safe: run() is called exactly once)
        static DEV_DET_CELL: static_cell::StaticCell<BasicInfoConfig> =
            static_cell::StaticCell::new();
        let dev_det = DEV_DET_CELL.init(dev_det(&config.device_info, serial_no));

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
            warn!(
                "Failed to load persisted state ({:?}), resetting KV store",
                e
            );
            stack
                .reset(&crypto, (&node, &handler), &mut kv)
                .await
                .unwrap();
            stack.startup(&crypto, &mut kv).await.unwrap();
        }

        // Wrap KV store as shared reference for concurrent access by Matter stack
        let kv = stack.matter().kv(kv);

        self.readiness.attach_matter(stack.matter());

        info!("Matter stack initialized, starting concurrent commissioning (BLE + WiFi coex)");

        let is_commissioned = has_active_controller_fabric(stack.matter(), FabricPolicy::default());
        self.readiness.set_paired(is_commissioned);
        if is_commissioned {
            self.state_sender.wifi_connecting();
        } else {
            self.state_sender.commissioning();
        }

        // Run the Matter stack forever.
        // Concurrent mode (`run_coex`): BLE and WiFi run simultaneously
        // via the coexistence driver. Required because sequential mode
        // (BLE then WiFi) breaks WiFi on ESP32-C6 — the radio doesn't
        // properly transition from BLE-only to WiFi-only.
        let mut monitor = ConnectionMonitor {
            state_sender: self.state_sender,
            readiness: self.readiness,
            reset: self.reset,
            matter: stack.matter(),
        };
        stack
            .run_coex(
                EmbassyWifi::new(
                    EspWifiDriver::new(config.peripherals.wifi, config.peripherals.bt),
                    weak_rand,
                    true,
                    stack,
                )
                .with_hostname(hostname),
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
/// Also monitors for decommissioning (primary controller fabric removed in Apple Home)
/// and triggers credential erasure and reboot.
///
/// Implements `UserTask`: runs alongside the Matter stack from startup.
/// Uses `NetifDiag` to check if any interface is operational, and
/// `NetChangeNotif` to wait for state changes. Signals `wifi_connecting`/`commissioning`
/// or `ecosystem_connecting`/`operational` when connectivity or pairing state changes.
struct ConnectionMonitor<'a, R: SystemResetDriver> {
    state_sender: SystemStateSender<'static>,
    readiness: &'static ClusterReadinessBarrier,
    reset: R,
    matter: &'a rs_matter::Matter<'a>,
}

impl<R: SystemResetDriver> UserTask for ConnectionMonitor<'_, R> {
    async fn run<S, N>(&mut self, _net_stack: S, netif: N) -> Result<(), rs_matter::error::Error>
    where
        S: rs_matter_embassy::stack::nal::NetStack,
        N: rs_matter::dm::clusters::gen_diag::NetifDiag + rs_matter::dm::networks::NetChangeNotif,
    {
        let fabric_policy = FabricPolicy::default();

        // Startup check: if the device booted with ONLY an orphaned Apple Home Hub fabric,
        // trigger decommissioning immediately to clear storage and start BLE advertising.
        let initially_commissioned = has_active_controller_fabric(self.matter, fabric_policy);
        if ConnectionState::orphan_only_boot(self.matter.has_fabrics(), initially_commissioned) {
            warn!("Device booted with only an orphaned hub fabric — triggering decommission reset");
            self.reset.erase_credentials(ResetScope::Ecosystem);
            self.reset.reboot();
        }

        let mut state = ConnectionState::new(initially_commissioned);

        loop {
            // Check if any network interface is operational
            let mut operational = false;
            netif.netifs(&mut |ni| {
                operational |= ni.operational;
                Ok(())
            })?;

            let commissioned = has_active_controller_fabric(self.matter, fabric_policy);
            self.readiness.set_paired(commissioned);

            let effects = state.step(operational, commissioned, self.readiness.is_ready());
            if effects.reset_readiness {
                self.readiness.reset();
            }
            if let Some(lifecycle) = effects.lifecycle {
                info!("network lifecycle -> {:?}", lifecycle);
                self.state_sender.send(lifecycle);
            }
            if effects.erase_ecosystem_and_reboot {
                warn!(
                    "Primary controller fabric removed — device decommissioned, triggering reset"
                );
                self.reset.erase_credentials(ResetScope::Ecosystem);
                self.reset.reboot();
            }

            // Wait for a network state change OR a periodic tick.
            let net_changed = pin!(netif.wait_changed());
            let tick = pin!(Timer::after(Duration::from_secs(3)));

            select(net_changed, tick).await;
        }
    }
}

/// Build device identity for the Basic Information cluster.
fn dev_det(info: &DeviceInfo, serial_no: &'static str) -> BasicInfoConfig<'static> {
    BasicInfoConfig {
        vid: VID,
        pid: PID,
        hw_ver: info.hw_ver,
        hw_ver_str: info.hw_ver_str,
        sw_ver: info.sw_ver,
        sw_ver_str: info.sw_ver_str,
        serial_no,
        unique_id: serial_no,
        vendor_name: info.vendor_name,
        product_name: info.product_name,
        device_name: info.device_name,
        max_subscription_max_interval_secs: Some(crate::config::MAX_SUBSCRIPTION_INTERVAL_SECS),
        ..BasicInfoConfig::new()
    }
}
