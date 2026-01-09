/*
   Smart Intercom - Door Actor

   Template actor that listens to UnlockPressed events
   and triggers door unlock via DoorDriver.
*/

#pragma once

#include "events/events.h"
#include <concepts>
#include <esp_log.h>

// ============================================================================
// DoorDriver Concept
// ============================================================================

/**
 * @brief Concept for door unlock drivers
 *
 * Driver must implement:
 *   void triggerUnlock()
 *   bool isUnlocking() const
 */
template <typename Driver>
concept DoorDriver = requires(Driver d) {
    d.triggerUnlock();
    { d.isUnlocking() } -> std::same_as<bool>;
};

// ============================================================================
// DoorActor Template
// ============================================================================

/**
 * @brief Door control actor
 *
 * Listens to UnlockPressed events and triggers hardware unlock via DoorDriver.
 *
 * @tparam Driver Type satisfying DoorDriver concept
 */
template <DoorDriver Driver>
class DoorActor {
   public:
    DoorActor(EventBus& bus, Driver& driver) : bus_(bus), driver_(driver) {}

    void init() {
        ESP_LOGI("DoorActor", "Initializing DoorActor");

        // Initialize the driver hardware
        driver_.init();

        // Subscribe to unlock events
        // Note: We need static callback, so we store instance pointer
        instance_ = this;
        bus_.unlockPressed.subscribe(&DoorActor::onUnlockPressedStatic);

        ESP_LOGI("DoorActor", "Subscribed to unlockPressed");
    }

   private:
    EventBus& bus_;
    Driver& driver_;
    static DoorActor* instance_;

    static void onUnlockPressedStatic(const UnlockPressedEvent& e) {
        if (instance_) {
            instance_->onUnlockPressed(e);
        }
    }

    void onUnlockPressed(const UnlockPressedEvent& e) {
        ESP_LOGI("DoorActor", "UnlockPressed received from %s (ts=%lld)", e.meta.source, (long long)e.meta.timestamp);

        if (driver_.isUnlocking()) {
            ESP_LOGW("DoorActor", "Unlock already in progress, ignoring");
            return;
        }

        ESP_LOGI("DoorActor", "Triggering hardware unlock");
        driver_.triggerUnlock();
    }
};

// Static member definition (must be in header for template)
template <DoorDriver Driver>
DoorActor<Driver>* DoorActor<Driver>::instance_ = nullptr;
