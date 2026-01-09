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
#include <app/server/CommissioningWindowManager.h>
#include <app/server/Server.h>

#include <common_macros.h>

static const char* TAG = "MatterActor";

using namespace esp_matter;
using namespace esp_matter::endpoint;
using namespace chip::app::Clusters;
using chip::ByteSpan;
using chip::EndpointId;
using chip::FabricIndex;
using chip::NodeId;
using chip::Optional;
using chip::app::Clusters::DoorLock::OperationErrorEnum;
using chip::app::DataModel::Nullable;

constexpr auto k_timeout_seconds = 300;

// Static member definitions
EventBus* MatterActor::bus_ = nullptr;
node_t* MatterActor::node_ = nullptr;
uint16_t MatterActor::lockEndpointId_ = 0;
uint16_t MatterActor::buzzEndpointId_ = 0;

// Global instance (defined in app_main.cpp)
extern MatterActor g_matterActor;

esp_err_t MatterActor::onAttributeUpdate(attribute::callback_type_t type, uint16_t endpoint_id,
                                         uint32_t cluster_id, uint32_t attribute_id,
                                         esp_matter_attr_val_t* val, void* priv_data) {
    // Attribute updates handled by actors via events
    return ESP_OK;
}

esp_err_t MatterActor::onIdentification(identification::callback_type_t type, uint16_t endpoint_id,
                                        uint8_t effect_id, uint8_t effect_variant, void* priv_data) {
    ESP_LOGI(TAG, "Identification callback: type: %u, effect: %u, variant: %u", type, effect_id, effect_variant);
    return ESP_OK;
}

void MatterActor::init(EventBus& bus) {
    ESP_LOGI(TAG, "Initializing MatterActor");

    bus_ = &bus;

    // Create Matter node (root of device tree)
    node::config_t node_config;
    node_ = node::create(&node_config, &MatterActor::onAttributeUpdate, &MatterActor::onIdentification);
    if (!node_) {
        ESP_LOGE(TAG, "Failed to create Matter node");
        return;
    }
    ESP_LOGI(TAG, "Matter node created");

    // Create Door Lock endpoint
    door_lock::config_t door_lock_config;
    endpoint_t* lock_endpoint = door_lock::create(node_, &door_lock_config, ENDPOINT_FLAG_NONE, nullptr);
    if (!lock_endpoint) {
        ESP_LOGE(TAG, "Failed to create door lock endpoint");
        return;
    }

    // Set auto-relock to 1 second (door springs back automatically)
    cluster_t* door_lock_cluster = cluster::get(lock_endpoint, DoorLock::Id);
    cluster::door_lock::attribute::create_auto_relock_time(door_lock_cluster, 1);

    lockEndpointId_ = endpoint::get_id(lock_endpoint);
    ESP_LOGI(TAG, "Door lock created with endpoint_id %d", lockEndpointId_);

    // Create Occupancy Sensor endpoint (appears as Motion Sensor in HomeKit)
    occupancy_sensor::config_t occupancy_config;
    occupancy_config.occupancy_sensing.occupancy = 0;  // Initially no motion
    endpoint_t* motion_endpoint = occupancy_sensor::create(node_, &occupancy_config, ENDPOINT_FLAG_NONE, nullptr);
    if (!motion_endpoint) {
        ESP_LOGE(TAG, "Failed to create occupancy sensor endpoint");
        return;
    }

    buzzEndpointId_ = endpoint::get_id(motion_endpoint);
    ESP_LOGI(TAG, "Ring sensor (occupancy/motion) created with endpoint_id %d", buzzEndpointId_);

    // Subscribe to events
    bus_->buzzStarted.subscribe(&MatterActor::onBuzzStarted);
    bus_->buzzEnded.subscribe(&MatterActor::onBuzzEnded);

    ESP_LOGI(TAG, "Subscribed to buzzStarted, buzzEnded");
}

esp_err_t MatterActor::start() {
    ESP_LOGI(TAG, "Starting Matter stack");

    esp_err_t err = esp_matter::start(&MatterActor::onDeviceEvent);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "Failed to start Matter, err:%d", err);
    }
    return err;
}

void MatterActor::onDeviceEvent(const chip::DeviceLayer::ChipDeviceEvent* event, intptr_t arg) {
    switch (event->Type) {
    case chip::DeviceLayer::DeviceEventType::kInterfaceIpAddressChanged:
        ESP_LOGI(TAG, "Interface IP Address changed");
        break;

    case chip::DeviceLayer::DeviceEventType::kCommissioningComplete:
        ESP_LOGI(TAG, "Commissioning complete");
        break;

    case chip::DeviceLayer::DeviceEventType::kFailSafeTimerExpired:
        ESP_LOGI(TAG, "Commissioning failed, fail safe timer expired");
        break;

    case chip::DeviceLayer::DeviceEventType::kCommissioningSessionStarted:
        ESP_LOGI(TAG, "Commissioning session started");
        break;

    case chip::DeviceLayer::DeviceEventType::kCommissioningSessionStopped:
        ESP_LOGI(TAG, "Commissioning session stopped");
        break;

    case chip::DeviceLayer::DeviceEventType::kCommissioningWindowOpened:
        ESP_LOGI(TAG, "Commissioning window opened");
        break;

    case chip::DeviceLayer::DeviceEventType::kCommissioningWindowClosed:
        ESP_LOGI(TAG, "Commissioning window closed");
        break;

    case chip::DeviceLayer::DeviceEventType::kFabricRemoved: {
        ESP_LOGI(TAG, "Fabric removed successfully");
        if (chip::Server::GetInstance().GetFabricTable().FabricCount() == 0) {
            chip::CommissioningWindowManager& commissionMgr =
                chip::Server::GetInstance().GetCommissioningWindowManager();
            constexpr auto kTimeoutSeconds = chip::System::Clock::Seconds16(k_timeout_seconds);
            if (!commissionMgr.IsCommissioningWindowOpen()) {
                CHIP_ERROR err = commissionMgr.OpenBasicCommissioningWindow(
                    kTimeoutSeconds, chip::CommissioningWindowAdvertisement::kDnssdOnly);
                if (err != CHIP_NO_ERROR) {
                    ESP_LOGE(TAG, "Failed to open commissioning window, err:%" CHIP_ERROR_FORMAT, err.Format());
                }
            }
        }
        break;
    }

    case chip::DeviceLayer::DeviceEventType::kFabricWillBeRemoved:
        ESP_LOGI(TAG, "Fabric will be removed");
        break;

    case chip::DeviceLayer::DeviceEventType::kFabricUpdated:
        ESP_LOGI(TAG, "Fabric is updated");
        break;

    case chip::DeviceLayer::DeviceEventType::kFabricCommitted:
        ESP_LOGI(TAG, "Fabric is committed");
        break;

    case chip::DeviceLayer::DeviceEventType::kBLEDeinitialized:
        ESP_LOGI(TAG, "BLE deinitialized and memory reclaimed");
        break;

    default:
        break;
    }
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

