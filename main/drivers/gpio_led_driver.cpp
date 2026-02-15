/*
   Smart Intercom - GPIO LED Driver Implementation

   Simple active-low red LED on GPIO.
*/

#include "gpio_led_driver.h"
#include <esp_log.h>

static const char* TAG = "GpioLedDriver";

GpioLedDriver::GpioLedDriver(const GpioLedConfig& config)
    : config_(config), gpioNum_(static_cast<gpio_num_t>(config.gpioPin)) {}

void GpioLedDriver::init() {
    ESP_LOGI(TAG, "Initializing GPIO LED (pin %d, active-low)", config_.gpioPin);

    gpio_config_t io_conf = {
        .pin_bit_mask = (1ULL << gpioNum_),
        .mode = GPIO_MODE_OUTPUT,
        .pull_up_en = GPIO_PULLUP_DISABLE,
        .pull_down_en = GPIO_PULLDOWN_DISABLE,
        .intr_type = GPIO_INTR_DISABLE,
    };
    ESP_ERROR_CHECK(gpio_config(&io_conf));

    // Start with LED off (HIGH for active-low)
    gpio_set_level(gpioNum_, 1);

    blinkTimer_ = xTimerCreate("led_blink", pdMS_TO_TICKS(100), pdFALSE, this, blinkTimerCallback);
    if (!blinkTimer_) {
        ESP_LOGE(TAG, "Failed to create blink timer");
    }

    ESP_LOGI(TAG, "GPIO LED initialized");
}

void GpioLedDriver::setLed(bool on) {
    // Active-low: LOW = on, HIGH = off
    gpio_set_level(gpioNum_, on ? 0 : 1);
}

void GpioLedDriver::setColor(uint8_t r, uint8_t g, uint8_t b) {
    // Any non-zero color component turns LED on
    setLed(r > 0 || g > 0 || b > 0);
}

void GpioLedDriver::yellow() {
    // Red-only LED: just turn on
    setLed(true);
}

void GpioLedDriver::green() {
    // Red-only LED: just turn on
    setLed(true);
}

void GpioLedDriver::red() {
    setLed(true);
}

void GpioLedDriver::off() {
    setLed(false);
}

void GpioLedDriver::blinkRed(uint8_t count, uint32_t onMs, uint32_t offMs) {
    if (count == 0 || !blinkTimer_) {
        return;
    }

    xTimerStop(blinkTimer_, 0);

    blinkRemaining_ = count;
    blinkOnMs_ = onMs;
    blinkOffMs_ = offMs;
    blinkPhase_ = true;

    red();
    xTimerChangePeriod(blinkTimer_, pdMS_TO_TICKS(onMs), 0);
    xTimerStart(blinkTimer_, 0);
}

void GpioLedDriver::blinkTimerCallback(TimerHandle_t timer) {
    auto* self = static_cast<GpioLedDriver*>(pvTimerGetTimerID(timer));

    if (self->blinkPhase_) {
        // End of ON phase
        self->off();
        self->blinkPhase_ = false;
        self->blinkRemaining_--;

        if (self->blinkRemaining_ > 0) {
            // More blinks to go - schedule OFF period
            xTimerChangePeriod(timer, pdMS_TO_TICKS(self->blinkOffMs_), 0);
            xTimerStart(timer, 0);
        } else {
            // Done blinking - return to steady on after final off period
            xTimerChangePeriod(timer, pdMS_TO_TICKS(self->blinkOffMs_), 0);
            xTimerStart(timer, 0);
            self->blinkRemaining_ = 255;  // Marker for "return to on"
        }
    } else {
        // End of OFF phase
        if (self->blinkRemaining_ == 255) {
            // Final off period done - return to steady on
            self->red();
        } else {
            // Start next ON phase
            self->red();
            self->blinkPhase_ = true;
            xTimerChangePeriod(timer, pdMS_TO_TICKS(self->blinkOnMs_), 0);
            xTimerStart(timer, 0);
        }
    }
}
