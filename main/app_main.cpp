/*
   Smart Intercom - Main Entry Point
*/

#include <esp_err.h>
#include <esp_log.h>
#include <nvs_flash.h>
#include <sdkconfig.h>
#if CONFIG_PM_ENABLE
#include <esp_pm.h>
#endif

#include <common_macros.h>
#include "config/app_priv.h"
#if CHIP_DEVICE_CONFIG_ENABLE_THREAD
#include <platform/ESP32/OpenthreadLauncher.h>
#endif

#include "app.h"
#include "services/led_service.h"

static const char* TAG = "app_main";

extern "C" void app_main() {
    esp_err_t err = ESP_OK;

    // Initialize LED first for status indication
    getLedService().init();
    getLedService().yellow();  // Yellow = initializing

    nvs_flash_init();

#if CONFIG_PM_ENABLE
    esp_pm_config_t pm_config = {.max_freq_mhz = CONFIG_ESP_DEFAULT_CPU_FREQ_MHZ,
                                 .min_freq_mhz = CONFIG_ESP_DEFAULT_CPU_FREQ_MHZ,
#if CONFIG_FREERTOS_USE_TICKLESS_IDLE
                                 .light_sleep_enable = true
#endif
    };
    err = esp_pm_configure(&pm_config);
#endif

    ESP_LOGI(TAG, "=== Smart Intercom Starting ===");
    ESP_LOGI(TAG, "Build: %s %s (endpoints=%d)", __DATE__, __TIME__, CONFIG_ESP_MATTER_MAX_DYNAMIC_ENDPOINT_COUNT);

#if CHIP_DEVICE_CONFIG_ENABLE_THREAD
    esp_openthread_platform_config_t config = {
        .radio_config = ESP_OPENTHREAD_DEFAULT_RADIO_CONFIG(),
        .host_config = ESP_OPENTHREAD_DEFAULT_HOST_CONFIG(),
        .port_config = ESP_OPENTHREAD_DEFAULT_PORT_CONFIG(),
    };
    set_openthread_platform_config(&config);
#endif

    err = getApp().init();
    if (err != ESP_OK) {
        getLedService().red();  // Red = initialization failed
        ESP_LOGE(TAG, "Failed to initialize app");
        return;
    }

    getLedService().green();  // Green = ready
    ESP_LOGI(TAG, "=== Smart Intercom Ready ===");
}
