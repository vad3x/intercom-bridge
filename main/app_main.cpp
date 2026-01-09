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

// Event bus (passed to actors via init)
static EventBus s_eventBus;

// Actors (static lifetime)
static GpioAdapter s_gpioAdapter;
static RingActor s_ringActor;
static IntercomActor s_intercomActor;
static LockActor s_lockActor;
MatterActor g_matterActor;  // Non-static for Matter SDK callbacks

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

    ESP_LOGI(TAG, "Initializing MatterActor...");

    // MatterActor creates node + endpoints and starts Matter
    err = g_matterActor.init(s_eventBus);
    ABORT_APP_ON_FAILURE(err == ESP_OK, ESP_LOGE(TAG, "Failed to initialize Matter"));

    // =========================================================================
    // Initialize Remaining Actors
    // Each actor subscribes to its own events in init()
    // (MatterActor already initialized above - creates endpoints before Matter starts)
    // =========================================================================

    // IntercomActor: business logic (subscribes: ringPressed, unlockPressed)
    s_intercomActor.init(s_eventBus, BUZZ_HOLD_MS);

    // LockActor: hardware unlock (subscribes: unlocked)
    s_lockActor.init(s_eventBus, &s_gpioAdapter);

    // RingActor: hardware buzz detection (no subscriptions, only emits)
    s_ringActor.init(s_eventBus, &s_gpioAdapter);

    ESP_LOGI(TAG, "All actors initialized");
    ESP_LOGI(TAG, "=== Smart Intercom Ready ===");
}
