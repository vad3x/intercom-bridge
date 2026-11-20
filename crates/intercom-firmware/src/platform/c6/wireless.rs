//! ESP32-C6 wireless transport and hardware TRNG driver.

use intercom_adapter_matter::{MatterStackStorage, MatterWirelessDriver};
use rs_matter::dm::{AsyncHandler, Node};
use rs_matter::persist::KvBlobStore;

use crate::platform;

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
        let wifi = self.wifi.take().expect("C6 WIFI already consumed");
        let (controller, stack, runner) =
            platform::init_esp_network(wifi, hostname, seed, enet_resources.stack_resources);
        self.wifi_controller = Some(controller);
        self.enet_stack = Some(stack);
        self.enet_buffers = Some(enet_resources.buffers);
        (stack, runner)
    }
}

impl MatterWirelessDriver for BoardWireless {
    type Rng = esp_hal::rng::Trng;

    async fn init_entropy(&mut self) -> Self::Rng {
        esp_hal::rng::Trng::try_new().expect("TrngSource initialized in wire_board")
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
            .expect("C6 Wi-Fi controller not initialized");
        let enet_stack = self.enet_stack.expect("C6 enet stack not initialized");
        let enet_buffers = self.enet_buffers.expect("C6 enet buffers not initialized");
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
