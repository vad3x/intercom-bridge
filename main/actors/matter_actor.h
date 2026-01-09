/*
   Smart Intercom - Matter Actor

   Handles Matter/HomeKit integration:
   - Creates Matter node, Door Lock and Occupancy Sensor endpoints
   - Listens to BuzzStarted/BuzzEnded → updates Occupancy Sensor (Motion in HomeKit)
   - Provides unlock()/lock() methods called by Matter callbacks
*/

#pragma once

#include "events/events.h"
#include <esp_matter.h>

/**
 * @brief Matter/HomeKit integration actor
 *
 * Creates Matter node/endpoints and bridges between event bus and Matter clusters.
 */
class MatterActor {
public:
    /**
     * @brief Initialize Matter actor - creates node, endpoints, starts Matter, subscribes to events
     * @param bus Reference to EventBus for subscribing and emitting
     * @return ESP_OK on success
     *
     * Creates: Matter node, Door Lock endpoint, Occupancy Sensor endpoint
     * Starts Matter stack, subscribes to: buzzStarted, buzzEnded
     */
    esp_err_t init(EventBus& bus);

    /**
     * @brief Called by Matter callbacks when user requests unlock
     * Emits UnlockPressed event
     */
    void unlock();

    /**
     * @brief Called by Matter callbacks when user requests lock
     * No-op for intercom (door auto-locks)
     */
    void lock();

private:
    static EventBus* bus_;
    static uint16_t lockEndpointId_;
    static uint16_t buzzEndpointId_;

    // Event handlers (called by EventBus)
    static void onBuzzStarted(const BuzzStartedEvent& e);
    static void onBuzzEnded(const BuzzEndedEvent& e);

    // Matter state updates
    static void updateLockState(bool unlocked);
    static void updateOccupancy(bool occupied);
};

