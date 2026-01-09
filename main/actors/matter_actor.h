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
    explicit MatterActor(EventBus& bus);

    /**
     * @brief Initialize Matter - creates node, endpoints, starts stack, subscribes to events
     * @return ESP_OK on success
     */
    esp_err_t init();

    void unlock();
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
