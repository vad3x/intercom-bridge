/*
   Smart Intercom - GPIO LED Driver

   Simple GPIO LED implementation (active-low red LED).
   LED is ON when GPIO is LOW, OFF when GPIO is HIGH.
*/

#pragma once

#include <cstdint>
#include <driver/gpio.h>
#include <freertos/FreeRTOS.h>
#include <freertos/timers.h>

struct GpioLedConfig {
    int gpioPin;
    uint8_t brightness;  // Unused for simple GPIO LED, kept for API compatibility
};

class GpioLedDriver {
   public:
    explicit GpioLedDriver(const GpioLedConfig& config);

    void init();
    void yellow();  // Red-only LED: same as on()
    void green();   // Red-only LED: same as on()
    void red();     // Turn on (active-low)
    void off();     // Turn off
    void blinkRed(uint8_t count, uint32_t onMs = 100, uint32_t offMs = 100);
    void setColor(uint8_t r, uint8_t g, uint8_t b);  // Any non-zero = on

   private:
    void setLed(bool on);
    static void blinkTimerCallback(TimerHandle_t timer);

    GpioLedConfig config_;
    gpio_num_t gpioNum_;
    TimerHandle_t blinkTimer_ = nullptr;

    // Blink state
    uint8_t blinkRemaining_ = 0;
    uint32_t blinkOnMs_ = 100;
    uint32_t blinkOffMs_ = 100;
    bool blinkPhase_ = false;
};
