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
#include <platform/CHIPDeviceEvent.h>

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
    static esp_matter::node_t* node_;
    static uint16_t lockEndpointId_;
    static uint16_t buzzEndpointId_;

    // Matter callbacks
    static esp_err_t onAttributeUpdate(esp_matter::attribute::callback_type_t type,
                                       uint16_t endpoint_id, uint32_t cluster_id,
                                       uint32_t attribute_id, esp_matter_attr_val_t* val,
                                       void* priv_data);
    static esp_err_t onIdentification(esp_matter::identification::callback_type_t type,
                                      uint16_t endpoint_id, uint8_t effect_id,
                                      uint8_t effect_variant, void* priv_data);
    static void onDeviceEvent(const chip::DeviceLayer::ChipDeviceEvent* event, intptr_t arg);

    // Event handlers (called by EventBus)
    static void onBuzzStarted(const BuzzStartedEvent& e);
    static void onBuzzEnded(const BuzzEndedEvent& e);

    // Matter state updates
    static void updateLockState(bool unlocked);
    static void updateOccupancy(bool occupied);
};

