/*
   Smart Intercom - Matter Access Control Driver

   Implements AccessControlDriver for Matter/HomeKit integration.
   Creates Door Lock and Occupancy Sensor endpoints.
*/

#pragma once

#include <cstdint>
#include <esp_err.h>
#include <esp_matter.h>
#include <freertos/FreeRTOS.h>
#include <freertos/timers.h>
#include <functional>

/**
 * @brief Configuration for Matter access control driver
 */
struct MatterAccessControlConfig {
    uint32_t ringHoldMs;  // How long occupancy stays active after ring
};

/**
 * @brief Matter-based access control driver
 *
 * Satisfies AccessControlDriver concept:
 *   - init(onUnlockDispatched, onLockDispatched) - stores callbacks
 *   - ringing() - updates occupancy sensor with timing
 *
 * Also creates Matter endpoints and handles SDK callbacks.
 */
class MatterAccessControlDriver {
   public:
    using DispatchCallback = std::function<void()>;

    /**
     * @brief Construct with configuration
     */
    explicit MatterAccessControlDriver(const MatterAccessControlConfig& config) : config_(config) {}

    /**
     * @brief Initialize Matter - creates endpoints, starts stack
     * @param onUnlockDispatched Called when Matter unlock command received
     * @param onLockDispatched Called when Matter lock command received
     */
    void init(DispatchCallback onUnlockDispatched, DispatchCallback onLockDispatched);

    /**
     * @brief Called when ring detected - updates occupancy sensor
     *
     * Sets occupancy to true, starts timer to clear after ringHoldMs
     */
    void ringing();

    // Called by Matter SDK callbacks
    void unlock();
    void lock();

    // Static instance for SDK callback access
    static MatterAccessControlDriver* instance_;

   private:
    MatterAccessControlConfig config_;
    DispatchCallback onUnlockDispatched_;
    DispatchCallback onLockDispatched_;

    uint16_t lockEndpointId_ = 0;
    uint16_t occupancyEndpointId_ = 0;
    TimerHandle_t ringTimer_ = nullptr;

    // Matter state updates
    void updateLockState(bool unlocked);
    void updateOccupancy(bool occupied);

    // Timer callback
    static void ringTimerCallback(TimerHandle_t timer);
};
