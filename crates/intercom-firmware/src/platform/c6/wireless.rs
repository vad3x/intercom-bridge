//! ESP32-C6 wireless transport and hardware TRNG driver.

use intercom_adapter_matter::{MatterStackStorage, MatterWirelessDriver};
use rs_matter::dm::{AsyncHandler, Node};
use rs_matter::persist::KvBlobStore;

use crate::platform;

pub struct BoardWireless {
    pub(crate) wifi: esp_hal::peripherals::WIFI<'static>,
    pub(crate) bt: esp_hal::peripherals::BT<'static>,
    pub(crate) rng: Option<esp_hal::peripherals::RNG<'static>>,
    pub(crate) adc1: Option<esp_hal::peripherals::ADC1<'static>>,
    pub(crate) _trng_source: Option<esp_hal::rng::TrngSource<'static>>,
}

impl MatterWirelessDriver for BoardWireless {
    type Rng = esp_hal::rng::Trng;

    async fn init_entropy(&mut self) -> Self::Rng {
        let rng = self.rng.take().expect("C6 RNG already consumed");
        let adc1 = self.adc1.take().expect("C6 ADC1 already consumed");
        self._trng_source = Some(esp_hal::rng::TrngSource::new(rng, adc1));
        esp_hal::rng::Trng::try_new().expect("TrngSource initialized above")
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
