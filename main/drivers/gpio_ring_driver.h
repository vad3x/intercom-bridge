/*
   Smart Intercom - GPIO Ring Driver

   Hardware driver for ring/buzz detection via optocoupler.
   Satisfies the RingDriver concept.
*/

#pragma once

#include <cstdint>
#include <freertos/FreeRTOS.h>
#include <freertos/task.h>
#include <freertos/timers.h>
#include <functional>

/**
 * @brief Configuration for GPIO ring driver
 */
struct GpioRingConfig {
    int gpioPin;          // GPIO pin for ring detection
    uint32_t debounceMs;  // Debounce time in milliseconds
    uint32_t cooldownMs;  // Cooldown between ring events
};

/**
 * @brief GPIO-based ring detection driver
 *
 * Detects ring signals from optocoupler.
 * Includes debouncing and cooldown logic.
 * Uses a dedicated task to handle ring events (avoids timer stack overflow).
 */
class GpioRingDriver {
   public:
    using OnRingCallback = std::function<void()>;

    /**
     * @brief Construct with configuration
     */
    explicit GpioRingDriver(const GpioRingConfig& config) : config_(config) {}

    /**
     * @brief Initialize the ring detection hardware
     * @param onRing Callback to invoke when valid ring detected
     */
    void init(OnRingCallback onRing);

    // Public for ISR/timer access
    TimerHandle_t debounceTimer_ = nullptr;
    TaskHandle_t ringTask_ = nullptr;
    volatile bool ringPending_ = false;
    volatile int64_t lastRingTime_ = 0;
    volatile bool ringValidated_ = false;  // Set by timer, cleared by task

    GpioRingConfig config_;
    OnRingCallback ringCallback_;
};
