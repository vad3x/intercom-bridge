/*
   Smart Intercom - GPIO Ring Driver Implementation

   Hardware interface for ring detection via optocoupler (HCPL-817).
*/

#include "drivers/gpio_ring_driver.h"
#include <driver/gpio.h>
#include <esp_log.h>
#include <esp_timer.h>

static const char* TAG = "GpioRingDriver";

// ============================================================================
// ISR Handler (must be file-scope for IRAM_ATTR)
// ============================================================================

static void IRAM_ATTR ring_isr_handler(void* arg) {
    auto* self = static_cast<GpioRingDriver*>(arg);
    self->ringPending_ = true;

    BaseType_t xHigherPriorityTaskWoken = pdFALSE;
    if (self->debounceTimer_) {
        xTimerStartFromISR(self->debounceTimer_, &xHigherPriorityTaskWoken);
    }

    if (xHigherPriorityTaskWoken) {
        portYIELD_FROM_ISR();
    }
}

// ============================================================================
// Public Methods
// ============================================================================

void GpioRingDriver::init(OnRingCallback onRing) {
    ESP_LOGI(TAG, "Initializing GpioRingDriver");
    ESP_LOGI(TAG, "  GPIO pin: %d (input from optocoupler)", config_.gpioPin);
    ESP_LOGI(TAG, "  Debounce: %lu ms", (unsigned long)config_.debounceMs);
    ESP_LOGI(TAG, "  Cooldown: %lu ms", (unsigned long)config_.cooldownMs);

    ringCallback_ = onRing;

    // Create debounce timer (pass 'this' via timer ID)
    debounceTimer_ =
        xTimerCreate("debounce", pdMS_TO_TICKS(config_.debounceMs), pdFALSE, this, [](TimerHandle_t timer) {
            auto* self = static_cast<GpioRingDriver*>(pvTimerGetTimerID(timer));

            if (!self->ringPending_) {
                return;
            }
            self->ringPending_ = false;

            // Check if signal is still LOW (active) after debounce
            int level = gpio_get_level((gpio_num_t)self->config_.gpioPin);
            if (level != 0) {
                ESP_LOGD(TAG, "Ring signal bounced");
                return;
            }

            // Check cooldown
            int64_t now = esp_timer_get_time() / 1000;
            if (self->lastRingTime_ > 0 && (now - self->lastRingTime_) < self->config_.cooldownMs) {
                ESP_LOGD(TAG, "Ring ignored (cooldown)");
                return;
            }

            // Valid ring detected!
            self->lastRingTime_ = now;
            ESP_LOGI(TAG, "=== RING DETECTED ===");

            if (self->ringCallback_) {
                self->ringCallback_();
            }
        });

    if (!debounceTimer_) {
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

    // Add ISR handler for ring detection (pass 'this' via arg)
    ret = gpio_isr_handler_add((gpio_num_t)config_.gpioPin, ring_isr_handler, this);
    if (ret != ESP_OK) {
        ESP_LOGE(TAG, "Failed to add ISR handler: %s", esp_err_to_name(ret));
    }

    ESP_LOGI(TAG, "Ring detection initialized");
}
