/*
   Smart Intercom - GPIO LED Driver

   Real WS2812 RGB LED implementation satisfying LedDriver concept.
*/

#pragma once

#include <cstdint>
#include <led_strip.h>
#include <freertos/FreeRTOS.h>
#include <freertos/timers.h>

struct GpioLedConfig {
    int gpioPin;
    uint8_t brightness;
};

class GpioLedDriver {
public:
    explicit GpioLedDriver(const GpioLedConfig& config);

    void init();
    void yellow();
    void green();
    void red();
    void off();
    void blinkRed(uint8_t count, uint32_t onMs = 100, uint32_t offMs = 100);
    void setColor(uint8_t r, uint8_t g, uint8_t b);

private:
    void setRawColor(uint8_t r, uint8_t g, uint8_t b);
    static void blinkTimerCallback(TimerHandle_t timer);

    GpioLedConfig config_;
    led_strip_handle_t ledStrip_ = nullptr;
    TimerHandle_t blinkTimer_ = nullptr;

    // Blink state
    uint8_t blinkRemaining_ = 0;
    uint32_t blinkOnMs_ = 100;
    uint32_t blinkOffMs_ = 100;
    bool blinkPhase_ = false;
};
