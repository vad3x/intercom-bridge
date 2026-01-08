/*
   Smart Intercom - Simplified Door Lock Manager

   Minimal implementation for intercom door release.
*/

#include "door_lock_manager.h"
#include <app_priv.h>
#include <esp_log.h>
#include <app-common/zap-generated/attributes/Accessors.h>

static const char *TAG = "lock_manager";

using namespace chip;
using namespace chip::app::Clusters;
using namespace chip::app::Clusters::DoorLock;

// Singleton instance
BoltLockManager BoltLockManager::sInstance;

CHIP_ERROR BoltLockManager::Init() {
    ESP_LOGI(TAG, "BoltLockManager initialized");
    return CHIP_NO_ERROR;
}

CHIP_ERROR BoltLockManager::InitLockState() {
    ESP_LOGI(TAG, "Initializing lock state");

    // Initialize GPIO for hardware control
    esp_err_t err = app_driver_gpio_init();
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "Failed to initialize GPIO: %s", esp_err_to_name(err));
        return CHIP_ERROR_INTERNAL;
    }

    // Set initial state to locked (intercom door is always locked by default)
    EndpointId endpointId = door_lock_endpoint_id;
    DoorLockServer::Instance().SetLockState(endpointId, DlLockState::kLocked);

    ESP_LOGI(TAG, "Lock state initialized to Locked on endpoint %d", endpointId);

    return CHIP_NO_ERROR;
}

bool BoltLockManager::Lock(EndpointId endpointId, const Optional<ByteSpan> &pin, OperationErrorEnum &err) {
    ESP_LOGI(TAG, "Lock command received (no-op - door auto-locks)");

    // Lock is a no-op for intercom - the door is always locked by its spring mechanism
    // Just update the Matter state
    return SetLockState(endpointId, DlLockState::kLocked, false);
}

bool BoltLockManager::Unlock(EndpointId endpointId, const Optional<ByteSpan> &pin, OperationErrorEnum &err) {
    ESP_LOGI(TAG, "Unlock command received - triggering relay pulse");

    // Unlock triggers the hardware relay
    return SetLockState(endpointId, DlLockState::kUnlocked, true);
}

bool BoltLockManager::SetLockState(EndpointId endpointId, DlLockState lockState, bool triggerHardware) {
    ESP_LOGI(TAG, "Setting lock state to %s (triggerHardware=%d)", LockStateToString(lockState), triggerHardware);

    // If unlocking and hardware trigger requested, pulse the relay
    if (lockState == DlLockState::kUnlocked && triggerHardware) {
        esp_err_t err = app_driver_unlock_pulse();
        if (err != ESP_OK) {
            ESP_LOGE(TAG, "Failed to trigger unlock pulse: %s", esp_err_to_name(err));
            // Still update state even if hardware fails (allows testing without hardware)
        }
    }

    // Update Matter lock state
    DoorLockServer::Instance().SetLockState(endpointId, lockState);

    return true;
}

const char *BoltLockManager::LockStateToString(DlLockState state) const {
    switch (state) {
    case DlLockState::kLocked:
        return "Locked";
    case DlLockState::kUnlocked:
        return "Unlocked";
    case DlLockState::kUnlatched:
        return "Unlatched";
    case DlLockState::kNotFullyLocked:
        return "NotFullyLocked";
    default:
        return "Unknown";
    }
}
