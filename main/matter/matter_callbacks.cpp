/*
   Smart Intercom - Matter SDK Callbacks

   Handles Matter Door Lock cluster callbacks.
   Routes commands to HomekitActor.
*/

#include "actors/homekit_actor.h"
#include <esp_log.h>
#include <lib/core/DataModelTypes.h>
#include <lib/core/Optional.h>
#include <app/data-model/Nullable.h>
#include <app/clusters/door-lock-server/door-lock-server.h>

static const char* TAG = "matter_callbacks";

using chip::ByteSpan;
using chip::EndpointId;
using chip::FabricIndex;
using chip::NodeId;
using chip::Optional;
using chip::app::Clusters::DoorLock::OperationErrorEnum;
using chip::app::DataModel::Nullable;

// Reference to HomekitActor (defined in app_main.cpp)
extern HomekitActor g_homekitActor;

/**
 * @brief Initialize Matter callbacks
 * Called from app_main to ensure callbacks are linked
 */
void matter_callbacks_init() {
    ESP_LOGI(TAG, "Matter callbacks initialized");
}

// emberAfDoorLockClusterInitCallback - cluster initialization callback
// Must match the declaration in esp_matter (C++ linkage)
__attribute__((used)) void emberAfDoorLockClusterInitCallback(chip::EndpointId endpoint) {
    ESP_LOGI(TAG, "Door Lock cluster init on endpoint %d", endpoint);
    DoorLockServer::Instance().InitServer(endpoint);
}

/**
 * @brief Handle Lock command
 * No-op for intercom - door is always locked by spring mechanism
 */
bool emberAfPluginDoorLockOnDoorLockCommand(EndpointId endpointId,
                                            const Nullable<FabricIndex>& fabricIdx,
                                            const Nullable<NodeId>& nodeId,
                                            const Optional<ByteSpan>& pinCode,
                                            OperationErrorEnum& err) {
    ESP_LOGI(TAG, "Lock command on endpoint %d", endpointId);
    g_homekitActor.lock();
    return true;
}

/**
 * @brief Handle Unlock command
 * This triggers the relay pulse to release the door
 */
bool emberAfPluginDoorLockOnDoorUnlockCommand(EndpointId endpointId,
                                              const Nullable<FabricIndex>& fabricIdx,
                                              const Nullable<NodeId>& nodeId,
                                              const Optional<ByteSpan>& pinCode,
                                              OperationErrorEnum& err) {
    ESP_LOGI(TAG, "Unlock command on endpoint %d", endpointId);
    g_homekitActor.unlock();
    return true;
}

/**
 * @brief Called when auto-relock timer fires
 * The Matter stack automatically calls this after auto_relock_time seconds
 */
void emberAfPluginDoorLockOnAutoRelock(EndpointId endpointId) {
    ESP_LOGI(TAG, "Auto-relock triggered on endpoint %d", endpointId);
    // The Matter stack handles setting the state back to Locked
    // No hardware action needed - door naturally re-locks
}

