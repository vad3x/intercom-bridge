/*
   Smart Intercom - Matter Actor Implementation

   Handles Matter/HomeKit integration:
   - Door Lock cluster callbacks
   - Occupancy sensor updates
   - Event bus integration
*/

#include "actors/matter_actor.h"
#include <esp_log.h>
#include <esp_matter.h>
#include <lib/core/DataModelTypes.h>
#include <lib/core/Optional.h>
#include <app/data-model/Nullable.h>
#include <app/clusters/door-lock-server/door-lock-server.h>

static const char* TAG = "MatterActor";

using namespace esp_matter;
using namespace chip::app::Clusters;
using chip::ByteSpan;
using chip::EndpointId;
using chip::FabricIndex;
using chip::NodeId;
using chip::Optional;
using chip::app::Clusters::DoorLock::OperationErrorEnum;
using chip::app::DataModel::Nullable;

// Static member definitions
EventBus* MatterActor::bus_ = nullptr;
uint16_t MatterActor::lockEndpointId_ = 0;
uint16_t MatterActor::buzzEndpointId_ = 0;

// Global instance (defined in app_main.cpp)
extern MatterActor g_matterActor;

void MatterActor::init(EventBus& bus, uint16_t lockEndpointId, uint16_t buzzEndpointId) {
    ESP_LOGI(TAG, "Initializing MatterActor (lockEp=%d, buzzEp=%d)",
             lockEndpointId, buzzEndpointId);

    bus_ = &bus;
    lockEndpointId_ = lockEndpointId;
    buzzEndpointId_ = buzzEndpointId;

    // Subscribe to events
    bus_->buzzStarted.subscribe(&MatterActor::onBuzzStarted);
    bus_->buzzEnded.subscribe(&MatterActor::onBuzzEnded);

    ESP_LOGI(TAG, "Subscribed to buzzStarted, buzzEnded");
}

void MatterActor::unlock() {
    ESP_LOGI(TAG, "Unlock requested - emitting UnlockPressed");

    // Update Matter lock state to unlocked
    updateLockState(true);

    // Emit event to trigger actual hardware unlock
    if (bus_) {
        bus_->emitUnlockPressed("MatterActor");
    }
}

void MatterActor::lock() {
    ESP_LOGI(TAG, "Lock requested (no-op - door auto-locks)");

    // Update Matter lock state to locked
    updateLockState(false);
}

void MatterActor::onBuzzStarted(const BuzzStartedEvent& e) {
    ESP_LOGI(TAG, "BuzzStarted received from %s (ts=%lld)",
             e.meta.source, (long long)e.meta.timestamp);

    updateOccupancy(true);  // Motion detected
}

void MatterActor::onBuzzEnded(const BuzzEndedEvent& e) {
    ESP_LOGI(TAG, "BuzzEnded received from %s (ts=%lld)",
             e.meta.source, (long long)e.meta.timestamp);

    updateOccupancy(false);  // Motion cleared
}

void MatterActor::updateLockState(bool unlocked) {
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

void MatterActor::updateOccupancy(bool occupied) {
    if (buzzEndpointId_ == 0) {
        ESP_LOGW(TAG, "Occupancy endpoint not initialized");
        return;
    }

    // OccupancySensing occupancy attribute is a bitmap, bit 0 = occupied
    uint8_t occupancyBitmap = occupied ? 1 : 0;
    ESP_LOGI(TAG, "Updating Matter occupancy to %s", occupied ? "Occupied" : "Unoccupied");

    esp_matter_attr_val_t val = esp_matter_uint8(occupancyBitmap);
    attribute::update(buzzEndpointId_,
                      OccupancySensing::Id,
                      OccupancySensing::Attributes::Occupancy::Id,
                      &val);
}

// ============================================================================
// Matter SDK Callbacks
// ============================================================================

/**
 * @brief Door Lock cluster initialization callback
 * Must match esp_matter declaration (C++ linkage, __attribute__((used)) to prevent stripping)
 */
__attribute__((used)) void emberAfDoorLockClusterInitCallback(chip::EndpointId endpoint) {
    ESP_LOGI(TAG, "Door Lock cluster init on endpoint %d", endpoint);
    DoorLockServer::Instance().InitServer(endpoint);
}

/**
 * @brief Handle Lock command from Matter
 * No-op for intercom - door is always locked by spring mechanism
 */
bool emberAfPluginDoorLockOnDoorLockCommand(EndpointId endpointId,
                                            const Nullable<FabricIndex>& fabricIdx,
                                            const Nullable<NodeId>& nodeId,
                                            const Optional<ByteSpan>& pinCode,
                                            OperationErrorEnum& err) {
    ESP_LOGI(TAG, "Lock command on endpoint %d", endpointId);
    g_matterActor.lock();
    return true;
}

/**
 * @brief Handle Unlock command from Matter
 * This triggers the relay pulse to release the door
 */
bool emberAfPluginDoorLockOnDoorUnlockCommand(EndpointId endpointId,
                                              const Nullable<FabricIndex>& fabricIdx,
                                              const Nullable<NodeId>& nodeId,
                                              const Optional<ByteSpan>& pinCode,
                                              OperationErrorEnum& err) {
    ESP_LOGI(TAG, "Unlock command on endpoint %d", endpointId);
    g_matterActor.unlock();
    return true;
}

/**
 * @brief Called when auto-relock timer fires
 * The Matter stack handles setting state back to Locked
 */
void emberAfPluginDoorLockOnAutoRelock(EndpointId endpointId) {
    ESP_LOGI(TAG, "Auto-relock triggered on endpoint %d", endpointId);
    // No hardware action needed - door naturally re-locks
}

