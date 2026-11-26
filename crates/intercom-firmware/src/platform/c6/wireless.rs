//! ESP32-C6 wireless transport, firmware-owned Embassy network stack, and TRNG driver.

extern crate alloc;

use core::cell::UnsafeCell;

use intercom_adapter_matter::{
    commissioning::select_wireless_boot_mode, commissioning::WirelessBootMode, MatterStackStorage,
    MatterWirelessDriver,
};
use portable_atomic::{AtomicBool, Ordering};
use rs_matter::dm::{AsyncHandler, Node};
use rs_matter::persist::KvBlobStore;
use rs_matter_embassy::matter::utils::init::InitMaybeUninit;
use static_cell::StaticCell;

struct SingleCoreStackCell(UnsafeCell<Option<rs_matter_embassy::enet::net::Stack<'static>>>);

// SAFETY: ESP32-C6 firmware runs on a single core and a single Embassy executor;
// `ENET_STACK_READY` Release/Acquire ordering publishes the single initialization write.
unsafe impl Sync for SingleCoreStackCell {}

static ENET_STACK: SingleCoreStackCell = SingleCoreStackCell(UnsafeCell::new(None));
static ENET_STACK_READY: AtomicBool = AtomicBool::new(false);

/// Return the firmware-owned Embassy network stack once initialized.
pub fn enet_stack() -> Option<rs_matter_embassy::enet::net::Stack<'static>> {
    if !ENET_STACK_READY.load(Ordering::Acquire) {
        return None;
    }
    // SAFETY: `ENET_STACK` is written once before `ENET_STACK_READY` is set to true.
    unsafe { *ENET_STACK.0.get() }
}

pub type EspWifiDriver = edf_matter::ipv6_compat::Ipv6CompatDriver<esp_radio::wifi::Interface>;

pub struct EnetMatterBuffers {
    pub udp_buffers: &'static mut rs_matter_embassy::enet::EnetMatterUdpBuffers,
    pub btp_gatt: &'static mut rs_matter_embassy::ble::BtpGattContext,
}

pub struct BoardWireless {
    wifi_controller: esp_radio::wifi::WifiController<'static>,
    bt: esp_hal::peripherals::BT<'static>,
    enet_stack: rs_matter_embassy::enet::net::Stack<'static>,
    enet_buffers: EnetMatterBuffers,
}

/// Initialize the `esp-radio` Wi-Fi controller and `embassy-net` stack once for ESP32-C6.
pub fn init_esp_network(
    wifi: esp_hal::peripherals::WIFI<'static>,
    bt: esp_hal::peripherals::BT<'static>,
    hostname: &str,
    seed: u64,
) -> (
    BoardWireless,
    rs_matter_embassy::enet::net::Stack<'static>,
    rs_matter_embassy::enet::net::Runner<'static, EspWifiDriver>,
) {
    static STACK_RESOURCES: StaticCell<rs_matter_embassy::enet::EnetMatterStackResources> =
        StaticCell::new();
    static BTP_GATT: StaticCell<rs_matter_embassy::ble::BtpGattContext> = StaticCell::new();

    let stack_resources =
        STACK_RESOURCES.init(rs_matter_embassy::enet::EnetMatterStackResources::new());
    let udp_buffers = alloc::boxed::Box::leak(alloc::boxed::Box::<
        rs_matter_embassy::enet::EnetMatterUdpBuffers,
    >::new_uninit())
    .init_with(rs_matter_embassy::enet::EnetMatterUdpBuffers::init());
    let btp_gatt = BTP_GATT.init(rs_matter_embassy::ble::BtpGattContext::new());

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
        edf_matter::ipv6_compat::wrap_ipv6_compat(esp_radio::wifi::Interface::station()),
        seed,
        stack_resources,
    );
    stack.set_config_v4(embassy_net::ConfigV4::Dhcp(
        crate::platform::dhcp_config_with_hostname(hostname),
    ));
    // SAFETY: called once during single-threaded startup before concurrent tasks poll.
    unsafe {
        *ENET_STACK.0.get() = Some(stack);
    }
    ENET_STACK_READY.store(true, Ordering::Release);

    let wireless = BoardWireless {
        wifi_controller: controller,
        bt,
        enet_stack: stack,
        enet_buffers: EnetMatterBuffers {
            udp_buffers,
            btp_gatt,
        },
    };

    (wireless, stack, runner)
}

impl MatterWirelessDriver for BoardWireless {
    type Rng = esp_hal::rng::Trng;

    async fn init_entropy(&mut self) -> Self::Rng {
        esp_hal::rng::Trng::try_new().expect("TrngSource must be initialized in main")
    }

    async fn run_stack<'a, const BUMP_SIZE: usize, C, H, S, const KB: usize, M>(
        self,
        stack: &'a MatterStackStorage<BUMP_SIZE>,
        weak_rand: C::WeakRand<'a>,
        is_commissioned: bool,
        crypto: &'a C,
        handler: (Node<'a>, H),
        kv: rs_matter::dm::persist::SharedKvBlobStore<'a, S, KB>,
        monitor: M,
    ) where
        C: rs_matter::crypto::Crypto,
        H: AsyncHandler,
        S: KvBlobStore,
        M: rs_matter_embassy::stack::UserTask,
    {
        let net_stack =
            rs_matter_embassy::enet::EnetStack::new(self.enet_stack, self.enet_buffers.udp_buffers);
        let netif = rs_matter_embassy::enet::EnetNetif::new(
            self.enet_stack,
            rs_matter_embassy::matter::dm::clusters::gen_diag::InterfaceTypeEnum::WiFi,
        );
        let net_ctl = rs_matter_embassy::wifi::esp::EspWifiController::new(self.wifi_controller)
            .with_wifi_config(
                rs_matter_embassy::wireless::esp::WifiConfig::new()
                    .with_scan_method(rs_matter_embassy::wireless::esp::ScanMethod::AllChannels)
                    .with_failure_retry_cnt(3),
            );
        let mdns = rs_matter_embassy::matter::transport::network::mdns::builtin::BuiltinMdns::new();
        let gatt = rs_matter_embassy::wireless::esp::EspGattPeripheral::new(
            self.bt,
            Some(weak_rand),
            self.enet_buffers.btp_gatt,
        );
        let wireless = rs_matter_embassy::stack::wireless::PreexistingWireless::new(
            net_stack, netif, net_ctl, mdns, gatt,
        );

        match select_wireless_boot_mode(is_commissioned) {
            WirelessBootMode::OperationalWifiOnly => {
                log::info!("Commissioned fabric present -> starting Wi-Fi-only operational stack");
                stack
                    .run_wifi_oper(wireless, crypto, handler, kv, monitor)
                    .await
                    .expect("Matter operational Wi-Fi stack fatal error");
            }
            WirelessBootMode::BleWifiCoex => {
                log::info!("Uncommissioned -> starting concurrent BLE + Wi-Fi commissioning");
                stack
                    .run_wifi(wireless, crypto, handler, kv, monitor)
                    .await
                    .expect("Matter coex stack fatal error");
            }
        }
    }
}
