/*
   Smart Intercom - HomeKit Actor Implementation
*/

#include "homekit_actor.h"
#include <esp_log.h>
#include <esp_matter.h>
#include <app/clusters/door-lock-server/door-lock-server.h>

static const char* TAG = "HomekitActor";

using namespace esp_matter;
using namespace chip::app::Clusters;

// Static member definitions
uint16_t HomekitActor::lockEndpointId_ = 0;
uint16_t HomekitActor::buzzEndpointId_ = 0;

void HomekitActor::init(uint16_t lockEndpointId, uint16_t buzzEndpointId) {
    ESP_LOGI(TAG, "Initializing HomekitActor (lockEp=%d, buzzEp=%d)",
             lockEndpointId, buzzEndpointId);

    lockEndpointId_ = lockEndpointId;
    buzzEndpointId_ = buzzEndpointId;

    // Subscribe to events
    g_eventBus.buzzStarted.subscribe(&HomekitActor::onBuzzStarted);
    g_eventBus.buzzEnded.subscribe(&HomekitActor::onBuzzEnded);

    ESP_LOGI(TAG, "Subscribed to buzzStarted, buzzEnded");
}

void HomekitActor::unlock() {
    ESP_LOGI(TAG, "Unlock requested - emitting UnlockPressed");

    // Update Matter lock state to unlocked
    updateLockState(true);

    // Emit event to trigger actual hardware unlock
    g_eventBus.emitUnlockPressed("HomekitActor");
}

void HomekitActor::lock() {
    ESP_LOGI(TAG, "Lock requested (no-op - door auto-locks)");

    // Update Matter lock state to locked
    updateLockState(false);
}

void HomekitActor::onBuzzStarted(const BuzzStartedEvent& e) {
    ESP_LOGI(TAG, "BuzzStarted received from %s (ts=%lld)",
             e.meta.source, (long long)e.meta.timestamp);

    updateBuzzSwitch(true);  // Press button
}

void HomekitActor::onBuzzEnded(const BuzzEndedEvent& e) {
    ESP_LOGI(TAG, "BuzzEnded received from %s (ts=%lld)",
             e.meta.source, (long long)e.meta.timestamp);

    updateBuzzSwitch(false);  // Release button
}

void HomekitActor::updateLockState(bool unlocked) {
    if (lockEndpointId_ == 0) {
        ESP_LOGW(TAG, "Lock endpoint not initialized");
        return;
    }

    auto lockState = unlocked
        ? DoorLock::DlLockState::kUnlocked
        : DoorLock::DlLockState::kLocked;

    ESP_LOGI(TAG, "Updating Matter lock state to %s",
             unlocked ? "Unlocked" : "Locked");

    DoorLockServer::Instance().SetLockState(lockEndpointId_, lockState);
}

void HomekitActor::updateBuzzSwitch(bool pressed) {
    if (buzzEndpointId_ == 0) {
        ESP_LOGW(TAG, "Buzz endpoint not initialized");
        return;
    }

    uint8_t position = pressed ? 1 : 0;
    ESP_LOGI(TAG, "Updating Matter switch position to %d", position);

    esp_matter_attr_val_t val = esp_matter_uint8(position);
    attribute::update(buzzEndpointId_,
                      Switch::Id,
                      Switch::Attributes::CurrentPosition::Id,
                      &val);
}

