/*
   Smart Intercom - Door Lock Callbacks
   
   Simplified callbacks for Matter Door Lock cluster.
   Only handles lock/unlock commands - no credentials, users, or schedules.
*/

#include <esp_log.h>
#include "door_lock_manager.h"
#include <lib/core/DataModelTypes.h>
#include <lib/core/Optional.h>
#include <app/data-model/Nullable.h>
#include <app/clusters/door-lock-server/door-lock-server.h>

static const char *TAG = "lock_callbacks";

using chip::EndpointId;
using chip::FabricIndex;
using chip::NodeId;
using chip::ByteSpan;
using chip::Optional;
using chip::app::Clusters::DoorLock::OperationErrorEnum;
using chip::app::DataModel::Nullable;

/**
 * @brief Initialize door lock
 * Called from app_main to ensure callbacks are linked
 */
void door_lock_init()
{
    ESP_LOGI(TAG, "Door lock callbacks initialized (simplified mode)");
}

/**
 * @brief Called when Matter stack initializes the door lock cluster
 */
void emberAfDoorLockClusterInitCallback(EndpointId endpoint)
{
    ESP_LOGI(TAG, "Door Lock cluster init on endpoint %d", endpoint);
    
    DoorLockServer::Instance().InitServer(endpoint);
    BoltLockMgr().Init();
    BoltLockMgr().InitLockState();
}

/**
 * @brief Handle Lock command
 * No-op for intercom - door is always locked by spring mechanism
 */
bool emberAfPluginDoorLockOnDoorLockCommand(EndpointId endpointId, 
                                            const Nullable<FabricIndex> & fabricIdx,
                                            const Nullable<NodeId> & nodeId, 
                                            const Optional<ByteSpan> & pinCode,
                                            OperationErrorEnum & err)
{
    ESP_LOGI(TAG, "Lock command on endpoint %d", endpointId);
    return BoltLockMgr().Lock(endpointId, pinCode, err);
}

/**
 * @brief Handle Unlock command
 * This triggers the relay pulse to release the door
 */
bool emberAfPluginDoorLockOnDoorUnlockCommand(EndpointId endpointId, 
                                              const Nullable<FabricIndex> & fabricIdx,
                                              const Nullable<NodeId> & nodeId, 
                                              const Optional<ByteSpan> & pinCode,
                                              OperationErrorEnum & err)
{
    ESP_LOGI(TAG, "Unlock command on endpoint %d", endpointId);
    return BoltLockMgr().Unlock(endpointId, pinCode, err);
}

/**
 * @brief Called when auto-relock timer fires
 * The Matter stack automatically calls this after auto_relock_time seconds
 */
void emberAfPluginDoorLockOnAutoRelock(EndpointId endpointId)
{
    ESP_LOGI(TAG, "Auto-relock triggered on endpoint %d", endpointId);
    // The Matter stack handles setting the state back to Locked
    // No hardware action needed - door naturally re-locks
}
