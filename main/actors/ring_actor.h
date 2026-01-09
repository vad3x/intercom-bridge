/*
   Smart Intercom - Ring Actor

   Template actor that listens to hardware ring detection via RingDriver
   and emits RingPressed events to the EventBus.
*/

#pragma once

#include "events/events.h"
#include <concepts>
#include <esp_log.h>
#include <functional>

// ============================================================================
// RingDriver Concept
// ============================================================================

/**
 * @brief Concept for ring detection drivers
 *
 * Driver must implement:
 *   void init(std::function<void()> onRing)
 */
template <typename Driver>
concept RingDriver = requires(Driver d, std::function<void()> onRing) { d.init(onRing); };

// ============================================================================
// RingActor Template
// ============================================================================

/**
 * @brief Ring detection actor
 *
 * Connects a RingDriver to the EventBus.
 * When hardware detects a ring, emits RingPressed event.
 *
 * @tparam Driver Type satisfying RingDriver concept
 */
template <RingDriver Driver>
class RingActor {
    static constexpr const char* TAG = "RingActor";

   public:
    RingActor(EventBus& bus, Driver& driver) : bus_(bus), driver_(driver) {}

    void init() {
        ESP_LOGI(TAG, "Initializing RingActor");
        driver_.init([this]() { onHardwareRing(); });
        ESP_LOGI(TAG, "Ring driver connected");
    }

   private:
    EventBus& bus_;
    Driver& driver_;

    void onHardwareRing() {
        ESP_LOGI(TAG, "Hardware ring detected - emitting Ringing");
        bus_.emitRinging(TAG);
    }
};
