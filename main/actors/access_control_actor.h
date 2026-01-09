/*
   Smart Intercom - Access Control Actor

   Template actor that bridges ring events to AccessControlDriver
   and handles unlock/lock dispatches from the driver.
*/

#pragma once

#include "events/events.h"
#include <concepts>
#include <esp_log.h>
#include <functional>

// ============================================================================
// AccessControlDriver Concept
// ============================================================================

/**
 * @brief Concept for access control drivers
 *
 * Driver must implement:
 *   void init(std::function<void()> onUnlockDispatched,
 *             std::function<void()> onLockDispatched)
 *   void ringing()  // Called when ring detected
 */
template <typename Driver>
concept AccessControlDriver =
    requires(Driver d, std::function<void()> onUnlockDispatched, std::function<void()> onLockDispatched) {
        d.init(onUnlockDispatched, onLockDispatched);
        d.ringing();
    };

// ============================================================================
// AccessControlActor Template
// ============================================================================

/**
 * @brief Access control actor
 *
 * Bridges between event bus and access control driver:
 * - On RingPressed: calls driver.ringing()
 * - On driver unlock dispatch: emits UnlockPressed
 *
 * @tparam Driver Type satisfying AccessControlDriver concept
 */
template <AccessControlDriver Driver>
class AccessControlActor {
   public:
    AccessControlActor(EventBus& bus, Driver& driver) : bus_(bus), driver_(driver) {}

    void init() {
        ESP_LOGI("AccessControlActor", "Initializing AccessControlActor");

        // Store instance for static callbacks
        instance_ = this;

        // Initialize driver with dispatch callbacks
        driver_.init([this]() { onUnlockDispatched(); }, [this]() { onLockDispatched(); });

        // Subscribe to ring events
        bus_.ringPressed.subscribe(&AccessControlActor::onRingPressedStatic);

        ESP_LOGI("AccessControlActor", "Subscribed to ringPressed");
    }

   private:
    EventBus& bus_;
    Driver& driver_;
    static AccessControlActor* instance_;

    // Static callback wrapper for event subscription
    static void onRingPressedStatic(const RingPressedEvent& e) {
        if (instance_) {
            instance_->onRingPressed(e);
        }
    }

    void onRingPressed(const RingPressedEvent& e) {
        ESP_LOGI("AccessControlActor", "RingPressed received from %s - calling driver.ringing()", e.meta.source);
        driver_.ringing();
    }

    void onUnlockDispatched() {
        ESP_LOGI("AccessControlActor", "Unlock dispatched from driver - emitting UnlockPressed");
        bus_.emitUnlockPressed("AccessControlActor");
    }

    void onLockDispatched() {
        ESP_LOGI("AccessControlActor", "Lock dispatched from driver (no-op)");
        // Lock is a no-op - door auto-locks
    }
};

// Static member definition (must be in header for template)
template <AccessControlDriver Driver>
AccessControlActor<Driver>* AccessControlActor<Driver>::instance_ = nullptr;
