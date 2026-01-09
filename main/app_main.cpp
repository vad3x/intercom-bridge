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

#include <esp_matter_console.h>
#include <esp_matter_ota.h>

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
MatterActor g_matterActor;  // Non-static for matter_callbacks.cpp access

#if CONFIG_ENABLE_ENCRYPTED_OTA
extern const char decryption_key_start[] asm("_binary_esp_image_encryption_key_pem_start");
extern const char decryption_key_end[] asm("_binary_esp_image_encryption_key_pem_end");

static const char* s_decryption_key = decryption_key_start;
static const uint16_t s_decryption_key_len = decryption_key_end - decryption_key_start;
#endif  // CONFIG_ENABLE_ENCRYPTED_OTA


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

    // MatterActor creates node + endpoints, then starts Matter
    g_matterActor.init(s_eventBus);

#if CHIP_DEVICE_CONFIG_ENABLE_THREAD
    /* Set OpenThread platform config */
    esp_openthread_platform_config_t config = {
        .radio_config = ESP_OPENTHREAD_DEFAULT_RADIO_CONFIG(),
        .host_config = ESP_OPENTHREAD_DEFAULT_HOST_CONFIG(),
        .port_config = ESP_OPENTHREAD_DEFAULT_PORT_CONFIG(),
    };
    set_openthread_platform_config(&config);
#endif

    err = g_matterActor.start();
    ABORT_APP_ON_FAILURE(err == ESP_OK, ESP_LOGE(TAG, "Failed to start Matter"));

    // =========================================================================
    // Initialize Remaining Actors
    // Each actor subscribes to its own events in init()
    // (MatterActor already initialized above - creates endpoints before Matter starts)
    // =========================================================================

    ESP_LOGI(TAG, "Initializing actors...");

    // IntercomActor: business logic (subscribes: ringPressed, unlockPressed)
    s_intercomActor.init(s_eventBus, BUZZ_HOLD_MS);

    // LockActor: hardware unlock (subscribes: unlocked)
    s_lockActor.init(s_eventBus, &s_gpioAdapter);

    // RingActor: hardware buzz detection (no subscriptions, only emits)
    s_ringActor.init(s_eventBus, &s_gpioAdapter);

    ESP_LOGI(TAG, "All actors initialized");
    ESP_LOGI(TAG, "=== Smart Intercom Ready ===");

#if CONFIG_ENABLE_ENCRYPTED_OTA
    err = esp_matter_ota_requestor_encrypted_init(s_decryption_key, s_decryption_key_len);
    ABORT_APP_ON_FAILURE(err == ESP_OK, ESP_LOGE(TAG, "Failed to initialized the encrypted OTA, err: %d", err));
#endif

#if CONFIG_ENABLE_CHIP_SHELL
    esp_matter::console::diagnostics_register_commands();
    esp_matter::console::wifi_register_commands();
    esp_matter::console::factoryreset_register_commands();
#if CONFIG_OPENTHREAD_CLI
    esp_matter::console::otcli_register_commands();
#endif
    esp_matter::console::init();
#endif
}
