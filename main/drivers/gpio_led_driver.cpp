/*
   Smart Intercom - GPIO LED Driver Implementation
*/

#include "gpio_led_driver.h"
#include <esp_log.h>

static const char* TAG = "GpioLedDriver";

GpioLedDriver::GpioLedDriver(const GpioLedConfig& config)
    : config_(config) {}

void GpioLedDriver::init() {
    ESP_LOGI(TAG, "Initializing GPIO LED (pin %d, brightness %d)", 
             config_.gpioPin, config_.brightness);

    led_strip_config_t strip_config = {
        .strip_gpio_num = config_.gpioPin,
        .max_leds = 1,
        .led_pixel_format = LED_PIXEL_FORMAT_GRB,
        .led_model = LED_MODEL_WS2812,
        .flags = {.invert_out = false},
    };
    led_strip_rmt_config_t rmt_config = {
        .clk_src = RMT_CLK_SRC_DEFAULT,
        .resolution_hz = 10 * 1000 * 1000,
        .mem_block_symbols = 64,
        .flags = {.with_dma = false},
    };
    ESP_ERROR_CHECK(led_strip_new_rmt_device(&strip_config, &rmt_config, &ledStrip_));

    blinkTimer_ = xTimerCreate("led_blink", pdMS_TO_TICKS(100), pdFALSE, this, blinkTimerCallback);
    if (!blinkTimer_) {
        ESP_LOGE(TAG, "Failed to create blink timer");
    }

    ESP_LOGI(TAG, "GPIO LED initialized");
}

void GpioLedDriver::setRawColor(uint8_t r, uint8_t g, uint8_t b) {
    if (ledStrip_) {
        led_strip_set_pixel(ledStrip_, 0, r, g, b);
        led_strip_refresh(ledStrip_);
    }
}

void GpioLedDriver::setColor(uint8_t r, uint8_t g, uint8_t b) {
    uint8_t scaledR = (r * config_.brightness) / 255;
    uint8_t scaledG = (g * config_.brightness) / 255;
    uint8_t scaledB = (b * config_.brightness) / 255;
    setRawColor(scaledR, scaledG, scaledB);
}

void GpioLedDriver::yellow() {
    setRawColor(config_.brightness, config_.brightness * 180 / 255, 0);
}

void GpioLedDriver::green() {
    setRawColor(0, config_.brightness, 0);
}

void GpioLedDriver::red() {
    setRawColor(config_.brightness, 0, 0);
}

void GpioLedDriver::off() {
    setRawColor(0, 0, 0);
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
        self->off();
        self->blinkPhase_ = false;
        self->blinkRemaining_--;

        if (self->blinkRemaining_ > 0) {
            xTimerChangePeriod(timer, pdMS_TO_TICKS(self->blinkOffMs_), 0);
            xTimerStart(timer, 0);
        } else {
            xTimerChangePeriod(timer, pdMS_TO_TICKS(self->blinkOffMs_), 0);
            xTimerStart(timer, 0);
            self->blinkRemaining_ = 255;
        }
    } else {
        if (self->blinkRemaining_ == 255) {
            self->green();
        } else {
            self->red();
            self->blinkPhase_ = true;
            xTimerChangePeriod(timer, pdMS_TO_TICKS(self->blinkOnMs_), 0);
            xTimerStart(timer, 0);
        }
    }
}
