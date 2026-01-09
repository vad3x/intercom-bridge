/*
   Smart Intercom - Lock Actor Implementation
*/

#include "lock_actor.h"
#include <esp_log.h>

static const char* TAG = "LockActor";

// Static member definition
LockOutputSocket* LockActor::socket_ = nullptr;

void LockActor::init(EventBus& bus, LockOutputSocket* socket) {
    ESP_LOGI(TAG, "Initializing LockActor");

    socket_ = socket;

    if (!socket_) {
        ESP_LOGW(TAG, "No LockOutputSocket provided");
    }

    // Subscribe to events
    bus.unlocked.subscribe(&LockActor::onUnlocked);

    ESP_LOGI(TAG, "Subscribed to unlocked");
}

void LockActor::onUnlocked(const UnlockedEvent& e) {
    ESP_LOGI(TAG, "Unlocked received from %s (ts=%lld)",
             e.meta.source, (long long)e.meta.timestamp);

    if (socket_) {
        if (socket_->isUnlocking()) {
            ESP_LOGW(TAG, "Unlock already in progress, ignoring");
            return;
        }

        ESP_LOGI(TAG, "Triggering hardware unlock");
        socket_->triggerUnlock();
    } else {
        ESP_LOGW(TAG, "No socket available, cannot unlock");
    }
}

