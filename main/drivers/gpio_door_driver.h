/*
   Smart Intercom - GPIO Door Driver

   Hardware driver for door unlock relay control.
   Satisfies the DoorDriver concept.
*/

#pragma once

#include <cstdint>

/**
 * @brief Configuration for GPIO door driver
 */
struct GpioDoorConfig {
    int gpioPin;           // GPIO pin for relay control
    uint32_t unlockPulseMs;  // Duration of unlock pulse in milliseconds
};

/**
 * @brief GPIO-based door unlock driver
 *
 * Controls PhotoMOS relay to release door latch.
 */
class GpioDoorDriver {
public:
    /**
     * @brief Construct with configuration
     */
    explicit GpioDoorDriver(const GpioDoorConfig& config) : config_(config) {}

    /**
     * @brief Initialize the door unlock hardware
     */
    void init();

    /**
     * @brief Trigger a timed unlock pulse
     */
    void triggerUnlock();

    /**
     * @brief Check if unlock is currently in progress
     */
    bool isUnlocking() const;

    // Called when unlock pulse ends (from timer callback)
    void onUnlockComplete();

private:
    GpioDoorConfig config_;
    volatile bool unlockInProgress_ = false;
};
