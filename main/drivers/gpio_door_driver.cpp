/*
   Smart Intercom - GPIO Door Driver Implementation

   Hardware interface for door unlock relay (PhotoMOS AQY212EHA).
*/

#include "drivers/gpio_door_driver.h"
#include "config/app_priv.h"
#include <driver/gpio.h>
#include <esp_log.h>
#include <freertos/FreeRTOS.h>
#include <freertos/timers.h>

static const char* TAG = "GpioDoorDriver";

// Static instance for timer callback access
GpioDoorDriver* GpioDoorDriver::instance_ = nullptr;

// Timer for unlock pulse
static TimerHandle_t s_unlock_timer = nullptr;

// ============================================================================
// Public Methods
// ============================================================================

void GpioDoorDriver::init() {
    ESP_LOGI(TAG, "Initializing GpioDoorDriver");
    ESP_LOGI(TAG, "  UNLOCK_GPIO: %d (output to PhotoMOS relay)", UNLOCK_GPIO);

    instance_ = this;

    // Create unlock pulse timer
    s_unlock_timer = xTimerCreate("unlock", pdMS_TO_TICKS(UNLOCK_PULSE_MS), pdFALSE, this, [](TimerHandle_t timer) {
        GpioDoorDriver* self = static_cast<GpioDoorDriver*>(pvTimerGetTimerID(timer));
        gpio_set_level((gpio_num_t)UNLOCK_GPIO, 0);
        self->onUnlockComplete();
    });

    if (!s_unlock_timer) {
        ESP_LOGE(TAG, "Failed to create unlock timer");
    }

    // Configure UNLOCK_GPIO as output
    gpio_config_t unlock_conf = {
        .pin_bit_mask = (1ULL << UNLOCK_GPIO),
        .mode = GPIO_MODE_OUTPUT,
        .pull_up_en = GPIO_PULLUP_DISABLE,
        .pull_down_en = GPIO_PULLDOWN_DISABLE,
        .intr_type = GPIO_INTR_DISABLE,
    };
    esp_err_t ret = gpio_config(&unlock_conf);
    if (ret != ESP_OK) {
        ESP_LOGE(TAG, "Failed to configure UNLOCK_GPIO: %s", esp_err_to_name(ret));
    }
    gpio_set_level((gpio_num_t)UNLOCK_GPIO, 0);  // Start LOW (relay off)

    ESP_LOGI(TAG, "Door unlock initialized");
}

void GpioDoorDriver::triggerUnlock() {
    if (unlockInProgress_) {
        ESP_LOGW(TAG, "Unlock already in progress");
        return;
    }

    ESP_LOGI(TAG, "Starting unlock pulse (%d ms)", UNLOCK_PULSE_MS);

    unlockInProgress_ = true;
    gpio_set_level((gpio_num_t)UNLOCK_GPIO, 1);  // Activate relay

    // Start timer to end pulse
    if (xTimerStart(s_unlock_timer, pdMS_TO_TICKS(100)) != pdPASS) {
        gpio_set_level((gpio_num_t)UNLOCK_GPIO, 0);
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
