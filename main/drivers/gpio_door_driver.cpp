/*
   Smart Intercom - GPIO Door Driver Implementation

   Hardware interface for door unlock relay (PhotoMOS AQY212EHA).
*/

#include "drivers/gpio_door_driver.h"
#include <driver/gpio.h>
#include <esp_log.h>
#include <freertos/FreeRTOS.h>
#include <freertos/timers.h>

static const char* TAG = "GpioDoorDriver";

// Timer for unlock pulse
static TimerHandle_t s_unlock_timer = nullptr;
static GpioDoorConfig s_config;  // Copy of config for timer callback

// ============================================================================
// Public Methods
// ============================================================================

void GpioDoorDriver::init() {
    ESP_LOGI(TAG, "Initializing GpioDoorDriver");
    ESP_LOGI(TAG, "  GPIO pin: %d (output to PhotoMOS relay)", config_.gpioPin);
    ESP_LOGI(TAG, "  Unlock pulse: %lu ms", (unsigned long)config_.unlockPulseMs);

    s_config = config_;  // Copy for timer callback

    // Create unlock pulse timer
    s_unlock_timer =
        xTimerCreate("unlock", pdMS_TO_TICKS(config_.unlockPulseMs), pdFALSE, nullptr, [](TimerHandle_t timer) {
            gpio_set_level((gpio_num_t)s_config.gpioPin, 0);
            if (auto* self = instance()) {
                self->onUnlockComplete();
            }
        });

    if (!s_unlock_timer) {
        ESP_LOGE(TAG, "Failed to create unlock timer");
    }

    // Configure GPIO as output
    gpio_config_t unlock_conf = {
        .pin_bit_mask = (1ULL << config_.gpioPin),
        .mode = GPIO_MODE_OUTPUT,
        .pull_up_en = GPIO_PULLUP_DISABLE,
        .pull_down_en = GPIO_PULLDOWN_DISABLE,
        .intr_type = GPIO_INTR_DISABLE,
    };
    esp_err_t ret = gpio_config(&unlock_conf);
    if (ret != ESP_OK) {
        ESP_LOGE(TAG, "Failed to configure GPIO: %s", esp_err_to_name(ret));
    }
    gpio_set_level((gpio_num_t)config_.gpioPin, 0);  // Start LOW (relay off)

    ESP_LOGI(TAG, "Door unlock initialized");
}

void GpioDoorDriver::triggerUnlock() {
    if (unlockInProgress_) {
        ESP_LOGW(TAG, "Unlock already in progress");
        return;
    }

    ESP_LOGI(TAG, "Starting unlock pulse (%lu ms)", (unsigned long)config_.unlockPulseMs);

    unlockInProgress_ = true;
    gpio_set_level((gpio_num_t)config_.gpioPin, 1);  // Activate relay

    // Start timer to end pulse
    if (xTimerStart(s_unlock_timer, pdMS_TO_TICKS(100)) != pdPASS) {
        gpio_set_level((gpio_num_t)config_.gpioPin, 0);
        unlockInProgress_ = false;
        ESP_LOGE(TAG, "Failed to start unlock timer");
    }
}

bool GpioDoorDriver::isUnlocking() const {
    return unlockInProgress_;
}

void GpioDoorDriver::onUnlockComplete() {
    unlockInProgress_ = false;
    ESP_LOGI(TAG, "Unlock pulse ended");
}
