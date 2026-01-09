/*
   Smart Intercom - Intercom Actor Implementation
*/

#include "actors/intercom_actor.h"
#include <esp_log.h>

static const char* TAG = "IntercomActor";

// Static member definitions
EventBus* IntercomActor::bus_ = nullptr;
uint32_t IntercomActor::buzzHoldMs_ = 4000;
TimerHandle_t IntercomActor::buzzTimer_ = nullptr;

// ============================================================================
// Public Methods
// ============================================================================

IntercomActor::IntercomActor(EventBus& bus, uint32_t buzzHoldMs) {
    bus_ = &bus;
    buzzHoldMs_ = buzzHoldMs;

    buzzTimer_ = xTimerCreate("buzz_hold", pdMS_TO_TICKS(buzzHoldMs_),
                              pdFALSE,  // One-shot
                              nullptr, &IntercomActor::buzzTimerCallback);

    if (!buzzTimer_) {
        ESP_LOGE(TAG, "Failed to create buzz timer");
    }
}

void IntercomActor::init() {
    ESP_LOGI(TAG, "Initializing IntercomActor (buzzHoldMs=%lu)", (unsigned long)buzzHoldMs_);

    bus_->ringPressed.subscribe(&IntercomActor::onRingPressed);
    bus_->unlockPressed.subscribe(&IntercomActor::onUnlockPressed);

    ESP_LOGI(TAG, "Subscribed to ringPressed, unlockPressed");
}

// ============================================================================
// Private Methods
// ============================================================================

void IntercomActor::onRingPressed(const RingPressedEvent& e) {
    ESP_LOGI(TAG, "RingPressed received from %s (ts=%lld)", e.meta.source, (long long)e.meta.timestamp);

    bus_->emitBuzzStarted("IntercomActor");

    if (buzzTimer_) {
        xTimerStop(buzzTimer_, 0);
        xTimerChangePeriod(buzzTimer_, pdMS_TO_TICKS(buzzHoldMs_), 0);
        xTimerStart(buzzTimer_, 0);
    }
}

void IntercomActor::onUnlockPressed(const UnlockPressedEvent& e) {
    ESP_LOGI(TAG, "UnlockPressed received from %s (ts=%lld)", e.meta.source, (long long)e.meta.timestamp);

    bus_->emitUnlocked("IntercomActor");
}

void IntercomActor::buzzTimerCallback(TimerHandle_t timer) {
    ESP_LOGI(TAG, "Buzz hold timer expired - emitting BuzzEnded");
    bus_->emitBuzzEnded("IntercomActor");
}
