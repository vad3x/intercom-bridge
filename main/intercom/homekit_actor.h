/*
   Smart Intercom - HomeKit Actor

   Handles Matter/HomeKit integration:
   - Listens to BuzzStarted/BuzzEnded → updates Occupancy Sensor (Motion in HomeKit)
   - Provides unlock()/lock() methods called by Matter callbacks
*/

#pragma once

#include "events.h"
#include <cstdint>

/**
 * @brief HomeKit/Matter integration actor
 *
 * Bridges between the event bus and Matter clusters.
 */
class HomekitActor {
public:
    /**
     * @brief Initialize HomeKit actor with endpoint IDs
     * @param bus Reference to EventBus for subscribing and emitting
     * @param lockEndpointId Matter endpoint ID for door lock
     * @param buzzEndpointId Matter endpoint ID for occupancy sensor (Motion in HomeKit)
     *
     * Subscribes to: buzzStarted, buzzEnded
     */
    void init(EventBus& bus, uint16_t lockEndpointId, uint16_t buzzEndpointId);

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

