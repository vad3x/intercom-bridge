/*
   Smart Intercom - GPIO Ring Driver Implementation

   Hardware interface for ring detection via optocoupler (HCPL-817).
*/

#include "drivers/gpio_ring_driver.h"
#include <driver/gpio.h>
#include <esp_log.h>
#include <esp_timer.h>
#include <freertos/FreeRTOS.h>
#include <freertos/timers.h>

static const char* TAG = "GpioRingDriver";

// State tracking
static volatile bool s_ring_pending = false;
static volatile int64_t s_last_ring_time = 0;
static TimerHandle_t s_debounce_timer = nullptr;
static GpioRingConfig s_config;  // Copy of config for callback access

// Forward declarations
static void IRAM_ATTR ring_isr_handler(void* arg);

// ============================================================================
// Public Methods
// ============================================================================

void GpioRingDriver::init(OnRingCallback onRing) {
    ESP_LOGI(TAG, "Initializing GpioRingDriver");
    ESP_LOGI(TAG, "  GPIO pin: %d (input from optocoupler)", config_.gpioPin);
    ESP_LOGI(TAG, "  Debounce: %lu ms", (unsigned long)config_.debounceMs);
    ESP_LOGI(TAG, "  Cooldown: %lu ms", (unsigned long)config_.cooldownMs);

    ringCallback_ = onRing;
    s_config = config_;  // Copy for callback access

    // Create debounce timer
    s_debounce_timer =
        xTimerCreate("debounce", pdMS_TO_TICKS(config_.debounceMs), pdFALSE, nullptr, [](TimerHandle_t timer) {
            if (!s_ring_pending) {
                return;
            }
            s_ring_pending = false;

            // Check if signal is still LOW (active) after debounce
            int level = gpio_get_level((gpio_num_t)s_config.gpioPin);
            if (level != 0) {
                ESP_LOGD(TAG, "Ring signal bounced");
                return;
            }

            // Check cooldown
            int64_t now = esp_timer_get_time() / 1000;
            if (s_last_ring_time > 0 && (now - s_last_ring_time) < s_config.cooldownMs) {
                ESP_LOGD(TAG, "Ring ignored (cooldown)");
                return;
            }

            // Valid ring detected!
            s_last_ring_time = now;
            ESP_LOGI(TAG, "=== RING DETECTED ===");

            if (auto* driver = GpioRingDriver::instance()) {
                if (driver->ringCallback_) {
                    driver->ringCallback_();
                }
            }
        });

    if (!s_debounce_timer) {
        ESP_LOGE(TAG, "Failed to create debounce timer");
    }

    // Configure GPIO as input with pull-up
    gpio_config_t ring_conf = {
        .pin_bit_mask = (1ULL << config_.gpioPin),
        .mode = GPIO_MODE_INPUT,
        .pull_up_en = GPIO_PULLUP_ENABLE,
        .pull_down_en = GPIO_PULLDOWN_DISABLE,
        .intr_type = GPIO_INTR_NEGEDGE,  // Falling edge (opto pulls LOW)
    };
    esp_err_t ret = gpio_config(&ring_conf);
    if (ret != ESP_OK) {
        ESP_LOGE(TAG, "Failed to configure GPIO: %s", esp_err_to_name(ret));
    }

    // Install GPIO ISR service
    ret = gpio_install_isr_service(0);
    if (ret != ESP_OK && ret != ESP_ERR_INVALID_STATE) {
        ESP_LOGE(TAG, "Failed to install GPIO ISR service: %s", esp_err_to_name(ret));
    }

    // Add ISR handler for ring detection
    ret = gpio_isr_handler_add((gpio_num_t)config_.gpioPin, ring_isr_handler, nullptr);
    if (ret != ESP_OK) {
        ESP_LOGE(TAG, "Failed to add ISR handler: %s", esp_err_to_name(ret));
    }

    ESP_LOGI(TAG, "Ring detection initialized");
}

// ============================================================================
// Static Helpers (file-scope)
// ============================================================================

static void IRAM_ATTR ring_isr_handler(void* arg) {
    s_ring_pending = true;

    BaseType_t xHigherPriorityTaskWoken = pdFALSE;
    if (s_debounce_timer) {
        xTimerStartFromISR(s_debounce_timer, &xHigherPriorityTaskWoken);
    }

    if (xHigherPriorityTaskWoken) {
        portYIELD_FROM_ISR();
    }
}
