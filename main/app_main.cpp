/*
   Smart Intercom - Main Application

   Event-driven actor architecture with Matter/HomeKit integration.
*/

#include <esp_err.h>
#include <esp_log.h>
#include <nvs_flash.h>
#if CONFIG_PM_ENABLE
#include <esp_pm.h>
#endif

#include <common_macros.h>
#include "config/app_priv.h"
#if CHIP_DEVICE_CONFIG_ENABLE_THREAD
#include <platform/ESP32/OpenthreadLauncher.h>
#endif

// Actor includes
#include "events/events.h"
#include "actors/ring_actor.h"
#include "actors/intercom_actor.h"
#include "actors/lock_actor.h"
#include "actors/matter_actor.h"
#include "hardware/gpio_adapter.h"


static const char* TAG = "app_main";

// Event bus (shared by all actors)
static EventBus s_eventBus;

// Hardware adapter
static GpioAdapter s_gpioAdapter;

// Actors (all receive EventBus via constructor)
static RingActor s_ringActor(s_eventBus);
static IntercomActor s_intercomActor(s_eventBus);
static LockActor s_lockActor(s_eventBus);
MatterActor g_matterActor(s_eventBus);  // Non-static for Matter SDK callbacks

extern "C" void app_main() {
    esp_err_t err = ESP_OK;

    /* Initialize the ESP NVS layer */
    nvs_flash_init();

#if CONFIG_PM_ENABLE
    esp_pm_config_t pm_config = {
        .max_freq_mhz = CONFIG_ESP_DEFAULT_CPU_FREQ_MHZ,
        .min_freq_mhz = CONFIG_ESP_DEFAULT_CPU_FREQ_MHZ,
#if CONFIG_FREERTOS_USE_TICKLESS_IDLE
        .light_sleep_enable = true
#endif
    };
    err = esp_pm_configure(&pm_config);
#endif

    ESP_LOGI(TAG, "=== Smart Intercom Starting ===");
    ESP_LOGI(TAG, "Architecture: Event-driven Actor Model");

#if CHIP_DEVICE_CONFIG_ENABLE_THREAD
    /* Set OpenThread platform config */
    esp_openthread_platform_config_t config = {
        .radio_config = ESP_OPENTHREAD_DEFAULT_RADIO_CONFIG(),
        .host_config = ESP_OPENTHREAD_DEFAULT_HOST_CONFIG(),
        .port_config = ESP_OPENTHREAD_DEFAULT_PORT_CONFIG(),
    };
    set_openthread_platform_config(&config);
#endif

    // Initialize actors (EventBus already passed via constructors)
    err = g_matterActor.init();
    ABORT_APP_ON_FAILURE(err == ESP_OK, ESP_LOGE(TAG, "Failed to initialize Matter"));

    s_intercomActor.init(BUZZ_HOLD_MS);
    s_lockActor.init(&s_gpioAdapter);
    s_ringActor.init(&s_gpioAdapter);

    ESP_LOGI(TAG, "All actors initialized");
    ESP_LOGI(TAG, "=== Smart Intercom Ready ===");
}
