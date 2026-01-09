/*
   Smart Intercom - Intercom Actor Implementation
*/

#include "intercom_actor.h"
#include <esp_log.h>

static const char* TAG = "IntercomActor";

// Static member definitions
uint32_t IntercomActor::buzzHoldMs_ = 4000;
TimerHandle_t IntercomActor::buzzTimer_ = nullptr;

void IntercomActor::init(uint32_t buzzHoldMs) {
    ESP_LOGI(TAG, "Initializing IntercomActor (buzzHoldMs=%lu)", (unsigned long)buzzHoldMs);

    buzzHoldMs_ = buzzHoldMs;

    // Create buzz hold timer (one-shot)
    buzzTimer_ = xTimerCreate("buzz_hold",
                              pdMS_TO_TICKS(buzzHoldMs_),
                              pdFALSE,  // One-shot
                              nullptr,
                              &IntercomActor::buzzTimerCallback);

    if (!buzzTimer_) {
        ESP_LOGE(TAG, "Failed to create buzz timer");
    }

    // Subscribe to events
    g_eventBus.ringPressed.subscribe(&IntercomActor::onRingPressed);
    g_eventBus.unlockPressed.subscribe(&IntercomActor::onUnlockPressed);

    ESP_LOGI(TAG, "Subscribed to ringPressed, unlockPressed");
}

void IntercomActor::onRingPressed(const RingPressedEvent& e) {
    ESP_LOGI(TAG, "RingPressed received from %s (ts=%lld)",
             e.meta.source, (long long)e.meta.timestamp);

    // Emit BuzzStarted
    g_eventBus.emitBuzzStarted("IntercomActor");

    // Start timer to emit BuzzEnded after timeout
    if (buzzTimer_) {
        // Reset timer if already running
        xTimerStop(buzzTimer_, 0);
        xTimerChangePeriod(buzzTimer_, pdMS_TO_TICKS(buzzHoldMs_), 0);
        xTimerStart(buzzTimer_, 0);
    }
}

void IntercomActor::onUnlockPressed(const UnlockPressedEvent& e) {
    ESP_LOGI(TAG, "UnlockPressed received from %s (ts=%lld)",
             e.meta.source, (long long)e.meta.timestamp);

    // Emit Unlocked to trigger the actual hardware unlock
    g_eventBus.emitUnlocked("IntercomActor");
}

void IntercomActor::buzzTimerCallback(TimerHandle_t timer) {
    ESP_LOGI(TAG, "Buzz hold timer expired - emitting BuzzEnded");
    g_eventBus.emitBuzzEnded("IntercomActor");
}

