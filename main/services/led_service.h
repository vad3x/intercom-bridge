/*
   Smart Intercom - LED Service
   
   Manages the RGB LED for status indication.
*/

#pragma once

#include <cstdint>
#include <led_strip.h>
#include <freertos/FreeRTOS.h>
#include <freertos/timers.h>

class LedService {
public:
    // Singleton access
    static LedService& instance();

    // Initialize the LED hardware
    void init();

    // Solid color states
    void yellow();  // Initializing
    void green();   // Ready/normal
    void red();     // Error
    void off();

    // Blink patterns (non-blocking, uses timer)
    void blinkRed(uint8_t count, uint32_t onMs = 100, uint32_t offMs = 100);

    // Set custom color (0-255 values, will be scaled by brightness)
    void setColor(uint8_t r, uint8_t g, uint8_t b);

private:
    LedService() = default;
    ~LedService() = default;
    LedService(const LedService&) = delete;
    LedService& operator=(const LedService&) = delete;

    void setRawColor(uint8_t r, uint8_t g, uint8_t b);
    static void blinkTimerCallback(TimerHandle_t timer);

    led_strip_handle_t ledStrip_ = nullptr;
    TimerHandle_t blinkTimer_ = nullptr;

    // Blink state
    uint8_t blinkCount_ = 0;
    uint8_t blinkRemaining_ = 0;
    uint32_t blinkOnMs_ = 100;
    uint32_t blinkOffMs_ = 100;
    bool blinkPhase_ = false;  // true = on, false = off

    // LED brightness at 10% (values scaled from 0-255 to 0-25)
    static constexpr uint8_t BRIGHTNESS = 25;
    static constexpr int LED_GPIO = 8;  // RGB LED on ESP32-C6-DevKitM-1
};

// Convenience function
inline LedService& getLedService() { return LedService::instance(); }

