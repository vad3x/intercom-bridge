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
    pub(crate) wifi: esp_hal::peripherals::WIFI<'static>,
    pub(crate) bt: esp_hal::peripherals::BT<'static>,
}

impl MatterWirelessDriver for BoardWireless {
    type Rng = S3EntropyRng;

    async fn init_entropy(&mut self) -> Self::Rng {
        S3EntropyRng
    }

    async fn run_stack<'a, C, H, S, const KB: usize, M>(
        self,
        stack: &'a MatterStackStorage,
        weak_rand: C::WeakRand<'a>,
        hostname: &'a str,
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
        assert!(
            platform::entropy::mark_radio_starting().is_ok(),
            "S3 entropy must be seeded before RF startup"
        );
        platform::run_esp_coex_or_oper_stack(
            self.wifi,
            self.bt,
            stack,
            weak_rand,
            hostname,
            is_commissioned,
            crypto,
            handler,
            kv,
            monitor,
        )
        .await;
    }
}
