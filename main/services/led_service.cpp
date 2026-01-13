/*
   Smart Intercom - LED Service Implementation
*/

#include "services/led_service.h"
#include <esp_log.h>

static const char* TAG = "LedService";

LedService& LedService::instance() {
    static LedService instance;
    return instance;
}

void LedService::init() {
    ESP_LOGI(TAG, "Initializing LED service (GPIO %d)", LED_GPIO);

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
    ESP_ERROR_CHECK(led_strip_new_rmt_device(&strip_config, &rmt_config, &ledStrip_));

    // Create blink timer (initially stopped)
    blinkTimer_ = xTimerCreate("led_blink", pdMS_TO_TICKS(100), pdFALSE, this, blinkTimerCallback);
    if (!blinkTimer_) {
        ESP_LOGE(TAG, "Failed to create blink timer");
    }

    ESP_LOGI(TAG, "LED service initialized");
}

void LedService::setRawColor(uint8_t r, uint8_t g, uint8_t b) {
    if (ledStrip_) {
        led_strip_set_pixel(ledStrip_, 0, r, g, b);
        led_strip_refresh(ledStrip_);
    }
}

void LedService::setColor(uint8_t r, uint8_t g, uint8_t b) {
    // Scale by brightness
    uint8_t scaledR = (r * BRIGHTNESS) / 255;
    uint8_t scaledG = (g * BRIGHTNESS) / 255;
    uint8_t scaledB = (b * BRIGHTNESS) / 255;
    setRawColor(scaledR, scaledG, scaledB);
}

void LedService::yellow() {
    setRawColor(BRIGHTNESS, BRIGHTNESS * 180 / 255, 0);
}

void LedService::green() {
    setRawColor(0, BRIGHTNESS, 0);
}

void LedService::red() {
    setRawColor(BRIGHTNESS, 0, 0);
}

void LedService::off() {
    setRawColor(0, 0, 0);
}

void LedService::blinkRed(uint8_t count, uint32_t onMs, uint32_t offMs) {
    if (count == 0 || !blinkTimer_) {
        return;
    }

    // Stop any existing blink
    xTimerStop(blinkTimer_, 0);

    // Setup blink state
    blinkCount_ = count;
    blinkRemaining_ = count;
    blinkOnMs_ = onMs;
    blinkOffMs_ = offMs;
    blinkPhase_ = true;  // Start with LED on

    // Turn on red and start timer
    red();
    xTimerChangePeriod(blinkTimer_, pdMS_TO_TICKS(onMs), 0);
    xTimerStart(blinkTimer_, 0);
}

void LedService::blinkTimerCallback(TimerHandle_t timer) {
    auto* self = static_cast<LedService*>(pvTimerGetTimerID(timer));

    if (self->blinkPhase_) {
        // Was on, turn off
        self->off();
        self->blinkPhase_ = false;
        self->blinkRemaining_--;

        if (self->blinkRemaining_ > 0) {
            // More blinks remaining, schedule next on
            xTimerChangePeriod(timer, pdMS_TO_TICKS(self->blinkOffMs_), 0);
            xTimerStart(timer, 0);
        } else {
            // All blinks done, return to green after off period
            xTimerChangePeriod(timer, pdMS_TO_TICKS(self->blinkOffMs_), 0);
            xTimerStart(timer, 0);
            self->blinkRemaining_ = 255;  // Special marker for final callback
        }
    } else {
        if (self->blinkRemaining_ == 255) {
            // Final callback - return to green
            self->green();
        } else {
            // Turn on for next blink
            self->red();
            self->blinkPhase_ = true;
            xTimerChangePeriod(timer, pdMS_TO_TICKS(self->blinkOnMs_), 0);
            xTimerStart(timer, 0);
        }
    }
}
