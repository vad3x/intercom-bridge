//! ESP32-S31 wireless transport and Wi-Fi-backed RNG driver.

use intercom_adapter_matter::{rand_core, MatterStackStorage, MatterWirelessDriver};
use log::info;
use rs_matter::dm::{AsyncHandler, Node};
use rs_matter::persist::KvBlobStore;
use rs_matter_embassy::wireless::{EmbassyWifi, PreexistingWifiRadio};

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
}

impl MatterWirelessDriver for BoardWireless {
    type Rng = S31RadioEntropyRng;

    async fn init_entropy(&mut self) -> Self::Rng {
        let wifi = self.wifi.take().expect("S31 WIFI already consumed");
        let mut controller = esp_radio::wifi::WifiController::new(
            wifi,
            esp_radio::wifi::ControllerConfig::default(),
        )
        .expect("Failed to create S31 Wi-Fi controller");
        controller
            .set_power_saving(esp_radio::wifi::PowerSaveMode::None)
            .expect("Failed to disable S31 Wi-Fi power saving");
        controller
            .set_max_tx_power(80)
            .expect("Failed to set S31 Wi-Fi transmit power");
        controller
            .set_config(&esp_radio::wifi::Config::Station(
                esp_radio::wifi::sta::StationConfig::default(),
            ))
            .expect("Failed to start S31 Wi-Fi radio");
        controller
            .scan_async(&esp_radio::wifi::scan::ScanConfig::default())
            .await
            .expect("Failed to initialize S31 Wi-Fi RF entropy source");
        self.wifi_controller = Some(controller);
        S31RadioEntropyRng(esp_hal::rng::Rng::new())
    }

    async fn run_stack<'a, C, H, S, const KB: usize, M>(
        mut self,
        stack: &'a MatterStackStorage,
        weak_rand: C::WeakRand<'a>,
        hostname: &'a str,
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
        let wifi = EmbassyWifi::new(
            PreexistingWifiRadio::new(
                esp_radio::wifi::Interface::station(),
                rs_matter_embassy::wifi::esp::EspWifiController::new(wifi_controller),
                (),
            ),
            weak_rand,
            false,
            stack,
        )
        .with_hostname(hostname);
        info!("Starting Wi-Fi-only Matter commissioning and operational stack");
        stack
            .run_wifi_oper(wifi, crypto, handler, kv, monitor)
            .await
            .expect("Matter operational Wi-Fi stack fatal error");
    }
}
