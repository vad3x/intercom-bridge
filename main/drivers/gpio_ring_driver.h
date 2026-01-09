/*
   Smart Intercom - GPIO Ring Driver

   Hardware driver for ring/buzz detection via optocoupler.
   Satisfies the RingDriver concept.
*/

#pragma once

#include <cstdint>
#include <functional>

/**
 * @brief Configuration for GPIO ring driver
 */
struct GpioRingConfig {
    int gpioPin;           // GPIO pin for ring detection
    uint32_t debounceMs;   // Debounce time in milliseconds
    uint32_t cooldownMs;   // Cooldown between ring events
};

/**
 * @brief GPIO-based ring detection driver
 *
 * Detects ring signals from optocoupler.
 * Includes debouncing and cooldown logic.
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

    // Called from ISR context after debounce
    static void onBuzzDetected();

    // Static instance for ISR access
    static GpioRingDriver* instance_;

private:
    GpioRingConfig config_;
    OnRingCallback ringCallback_;
};
