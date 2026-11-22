//! ESP32-S31 wireless transport and Wi-Fi-backed RNG driver.

use intercom_adapter_matter::{rand_core, MatterStackStorage, MatterWirelessDriver};
use log::info;
use rs_matter::dm::{AsyncHandler, Node};
use rs_matter::persist::KvBlobStore;

use crate::platform;

#[derive(Clone, Copy)]
pub struct S31RadioEntropyRng(pub(crate) esp_hal::rng::Rng);

impl rand_core::RngCore for S31RadioEntropyRng {
    fn next_u32(&mut self) -> u32 {
        self.0.random()
    }

    fn next_u64(&mut self) -> u64 {
        let mut bytes = [0; 8];
        self.0.read(&mut bytes);
        u64::from_le_bytes(bytes)
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.0.read(dest);
    }
}

impl rand_core::CryptoRng for S31RadioEntropyRng {}

pub struct BoardWireless {
    pub(crate) wifi: Option<esp_hal::peripherals::WIFI<'static>>,
    pub(crate) wifi_controller: Option<esp_radio::wifi::WifiController<'static>>,
    pub(crate) enet_stack: Option<rs_matter_embassy::enet::net::Stack<'static>>,
    pub(crate) enet_buffers: Option<platform::EnetMatterBuffers>,
}

impl BoardWireless {
    pub fn init_network(
        &mut self,
        hostname: &str,
        seed: u64,
        enet_resources: platform::EnetResources,
    ) -> (
        rs_matter_embassy::enet::net::Stack<'static>,
        rs_matter_embassy::enet::net::Runner<'static, platform::EspWifiDriver>,
    ) {
        let wifi = self.wifi.take().expect("S31 WIFI already consumed");
        let (controller, stack, runner) =
            platform::init_esp_network(wifi, hostname, seed, enet_resources.stack_resources);
        self.wifi_controller = Some(controller);
        self.enet_stack = Some(stack);
        self.enet_buffers = Some(enet_resources.buffers);
        (stack, runner)
    }
}

impl MatterWirelessDriver for BoardWireless {
    type Rng = S31RadioEntropyRng;

    async fn init_entropy(&mut self) -> Self::Rng {
        let controller = self
            .wifi_controller
            .as_mut()
            .expect("S31 Wi-Fi controller not initialized");
        controller
            .set_config(&esp_radio::wifi::Config::Station(
                esp_radio::wifi::sta::StationConfig::default(),
            ))
            .expect("Failed to start S31 Wi-Fi radio");
        controller
            .scan_async(&esp_radio::wifi::scan::ScanConfig::default())
            .await
            .expect("Failed to initialize S31 Wi-Fi RF entropy source");
        S31RadioEntropyRng(esp_hal::rng::Rng::new())
    }

    async fn run_stack<'a, const BUMP_SIZE: usize, C, H, S, const KB: usize, M>(
        mut self,
        stack: &'a MatterStackStorage<BUMP_SIZE>,
        _weak_rand: C::WeakRand<'a>,
        _is_commissioned: bool,
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
        let wifi_controller = self
            .wifi_controller
            .take()
            .expect("S31 Wi-Fi controller not initialized");
        let enet_stack = self.enet_stack.expect("S31 enet stack not initialized");
        let enet_buffers = self.enet_buffers.expect("S31 enet buffers not initialized");
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
        let mdns = rs_matter_embassy::matter::transport::network::mdns::builtin::BuiltinMdns::new();
        let wireless = rs_matter_embassy::stack::wireless::PreexistingWireless::new(
            net_stack,
            netif,
            net_ctl,
            mdns,
            (),
        );
        info!("Starting Wi-Fi-only Matter commissioning and operational stack");
        stack
            .run_wifi_oper(wireless, crypto, handler, kv, monitor)
            .await
            .expect("Matter operational Wi-Fi stack fatal error");
    }
}
