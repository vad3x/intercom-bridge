/*
   Smart Intercom - Main Entry Point
*/

#include <esp_err.h>
#include <esp_log.h>
#include <nvs_flash.h>
#include <sdkconfig.h>
#include <led_strip.h>
#if CONFIG_PM_ENABLE
#include <esp_pm.h>
#endif

#include <common_macros.h>
#include "config/app_priv.h"
#if CHIP_DEVICE_CONFIG_ENABLE_THREAD
#include <platform/ESP32/OpenthreadLauncher.h>
#endif

#include "app.h"

static const char* TAG = "app_main";

// RGB LED on ESP32-C6-DevKitM-1 is on GPIO8
#define LED_GPIO 8

static led_strip_handle_t s_led_strip = nullptr;

static void led_init() {
    led_strip_config_t strip_config = {
        .strip_gpio_num = LED_GPIO,
        .max_leds = 1,
        .led_pixel_format = LED_PIXEL_FORMAT_GRB,
        .led_model = LED_MODEL_WS2812,
        .flags = {.invert_out = false},
    };
    led_strip_rmt_config_t rmt_config = {
        .clk_src = RMT_CLK_SRC_DEFAULT,
        .resolution_hz = 10 * 1000 * 1000,  // 10 MHz
        .mem_block_symbols = 64,
        .flags = {.with_dma = false},
    };
    ESP_ERROR_CHECK(led_strip_new_rmt_device(&strip_config, &rmt_config, &s_led_strip));
}

static void led_set_color(uint8_t r, uint8_t g, uint8_t b) {
    if (s_led_strip) {
        led_strip_set_pixel(s_led_strip, 0, r, g, b);
        led_strip_refresh(s_led_strip);
    }
}

// LED brightness at 10% (values scaled from 0-255 to 0-25)
static constexpr uint8_t LED_BRIGHTNESS = 25;  // 10% of 255

static void led_yellow() { led_set_color(LED_BRIGHTNESS, LED_BRIGHTNESS * 180 / 255, 0); }
static void led_green() { led_set_color(0, LED_BRIGHTNESS, 0); }
static void led_red() { led_set_color(LED_BRIGHTNESS, 0, 0); }
static void led_off() { led_set_color(0, 0, 0); }

extern "C" void app_main() {
    esp_err_t err = ESP_OK;

    // Initialize LED first for status indication
    led_init();
    led_yellow();  // Yellow = initializing

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
        led_red();  // Red = initialization failed
        ESP_LOGE(TAG, "Failed to initialize app");
        return;
    }

    led_green();  // Green = ready
    ESP_LOGI(TAG, "=== Smart Intercom Ready ===");
}
