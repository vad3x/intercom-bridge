//! ESP32-S3 wireless transport and seeded global entropy driver.

use intercom_adapter_matter::{rand_core, MatterStackStorage, MatterWirelessDriver};
use rs_matter::dm::{AsyncHandler, Node};
use rs_matter::persist::KvBlobStore;

use crate::platform;

#[derive(Clone, Copy)]
pub struct S3EntropyRng;

impl rand_core::RngCore for S3EntropyRng {
    fn next_u32(&mut self) -> u32 {
        let mut bytes = [0; 4];
        self.fill_bytes(&mut bytes);
        u32::from_le_bytes(bytes)
    }

    fn next_u64(&mut self) -> u64 {
        let mut bytes = [0; 8];
        self.fill_bytes(&mut bytes);
        u64::from_le_bytes(bytes)
    }

    fn fill_bytes(&mut self, destination: &mut [u8]) {
        assert!(
            platform::entropy::fill_global(destination).is_ok(),
            "S3 secure entropy is not initialized"
        );
    }
}

impl rand_core::CryptoRng for S3EntropyRng {}

pub struct BoardWireless {
    pub(crate) wifi: Option<esp_hal::peripherals::WIFI<'static>>,
    pub(crate) bt: esp_hal::peripherals::BT<'static>,
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
        rs_matter_embassy::enet::net::Runner<
            'static,
            rs_matter_embassy::enet::MatterEnetDriver<esp_radio::wifi::Interface>,
        >,
    ) {
        assert!(
            platform::entropy::mark_radio_starting().is_ok(),
            "S3 entropy must be seeded before RF startup"
        );
        let wifi = self.wifi.take().expect("S3 WIFI already consumed");
        let (controller, stack, runner) =
            platform::init_esp_network(wifi, hostname, seed, enet_resources.stack_resources);
        self.wifi_controller = Some(controller);
        self.enet_stack = Some(stack);
        self.enet_buffers = Some(enet_resources.buffers);
        (stack, runner)
    }
}

impl MatterWirelessDriver for BoardWireless {
    type Rng = S3EntropyRng;

    async fn init_entropy(&mut self) -> Self::Rng {
        S3EntropyRng
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
        let wifi_controller = self
            .wifi_controller
            .expect("S3 Wi-Fi controller not initialized");
        let enet_stack = self.enet_stack.expect("S3 enet stack not initialized");
        let enet_buffers = self.enet_buffers.expect("S3 enet buffers not initialized");
        platform::run_esp_preexisting_stack(
            wifi_controller,
            self.bt,
            enet_stack,
            enet_buffers,
            stack,
            weak_rand,
            is_commissioned,
            crypto,
            handler,
            kv,
            monitor,
        )
        .await;
    }
}
