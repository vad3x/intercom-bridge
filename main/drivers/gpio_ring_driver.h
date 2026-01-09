/*
   Smart Intercom - GPIO Ring Driver

   Hardware driver for ring/buzz detection via optocoupler.
   Satisfies the RingDriver concept.
*/

#pragma once

#include <functional>

/**
 * @brief GPIO-based ring detection driver
 *
 * Detects ring signals from optocoupler on BUZZ_GPIO.
 * Includes debouncing and cooldown logic.
 */
class GpioRingDriver {
   public:
    using OnRingCallback = std::function<void()>;

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
    OnRingCallback ringCallback_;
};
