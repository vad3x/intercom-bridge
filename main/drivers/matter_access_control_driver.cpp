/*
   Smart Intercom - Matter Access Control Driver Implementation

   Handles Matter/HomeKit integration:
   - Door Lock cluster
   - Occupancy sensor for ring detection
   - SDK callbacks
*/

#include "drivers/matter_access_control_driver.h"
#include <app/clusters/door-lock-server/door-lock-server.h>
#include <app/data-model/Nullable.h>
#include <app/server/CommissioningWindowManager.h>
#include <app/server/Server.h>
#include <common_macros.h>
#include <esp_log.h>
#include <esp_matter.h>
#include <esp_matter_console.h>
#include <esp_matter_ota.h>
#include <lib/core/DataModelTypes.h>
#include <lib/core/Optional.h>

static const char* TAG = "MatterDriver";

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

// File-scope state
static node_t* s_node = nullptr;

#if CONFIG_ENABLE_ENCRYPTED_OTA
extern const char decryption_key_start[] asm("_binary_esp_image_encryption_key_pem_start");
extern const char decryption_key_end[] asm("_binary_esp_image_encryption_key_pem_end");
static const char* s_decryption_key = decryption_key_start;
static const uint16_t s_decryption_key_len = decryption_key_end - decryption_key_start;
#endif

// Forward declarations of file-scope callbacks
static esp_err_t onAttributeUpdate(attribute::callback_type_t type, uint16_t endpoint_id, uint32_t cluster_id,
                                   uint32_t attribute_id, esp_matter_attr_val_t* val, void* priv_data);
static esp_err_t onIdentification(identification::callback_type_t type, uint16_t endpoint_id, uint8_t effect_id,
                                  uint8_t effect_variant, void* priv_data);
static void onDeviceEvent(const chip::DeviceLayer::ChipDeviceEvent* event, intptr_t arg);

// ============================================================================
// Public Methods
// ============================================================================

void MatterAccessControlDriver::init(DispatchCallback onUnlockDispatched, DispatchCallback onLockDispatched) {
    ESP_LOGI(TAG, "Initializing MatterAccessControlDriver");

    onUnlockDispatched_ = onUnlockDispatched;
    onLockDispatched_ = onLockDispatched;

    // Create ring timer for occupancy timeout
    ringTimer_ = xTimerCreate("ring_hold", pdMS_TO_TICKS(config_.ringHoldMs), pdFALSE, nullptr, ringTimerCallback);
    if (!ringTimer_) {
        ESP_LOGE(TAG, "Failed to create ring timer");
    }

    // Create Matter node (root of device tree)
    node::config_t node_config;
    // Set device name that appears in Apple Home
    snprintf(node_config.root_node.basic_information.node_label, 
             sizeof(node_config.root_node.basic_information.node_label),
             "Smart Intercom");
    s_node = node::create(&node_config, onAttributeUpdate, onIdentification);
    if (!s_node) {
        ESP_LOGE(TAG, "Failed to create Matter node");
        return;
    }
    ESP_LOGI(TAG, "Matter node created");

    // Create Door Lock endpoint
    door_lock::config_t door_lock_config;
    endpoint_t* lock_endpoint = door_lock::create(s_node, &door_lock_config, ENDPOINT_FLAG_NONE, nullptr);
    if (!lock_endpoint) {
        ESP_LOGE(TAG, "Failed to create door lock endpoint");
        return;
    }

    // Set auto-relock time (door springs back automatically, this is Matter state only)
    cluster_t* door_lock_cluster = cluster::get(lock_endpoint, DoorLock::Id);
    cluster::door_lock::attribute::create_auto_relock_time(door_lock_cluster, config_.autoRelockSec);

    lockEndpointId_ = endpoint::get_id(lock_endpoint);
    ESP_LOGI(TAG, "Door lock created with endpoint_id %d", lockEndpointId_);

    // TODO: Re-enable occupancy sensor after commissioning works
    // Temporarily disabled to test if Apple accepts simple door lock
    ESP_LOGI(TAG, "Occupancy sensor disabled for commissioning test");

    // Start Matter stack
    ESP_LOGI(TAG, "Starting Matter stack");
    esp_err_t err = esp_matter::start(onDeviceEvent);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "Failed to start Matter, err:%d", err);
        return;
    }

#if CONFIG_ENABLE_ENCRYPTED_OTA
    err = esp_matter_ota_requestor_encrypted_init(s_decryption_key, s_decryption_key_len);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "Failed to initialize encrypted OTA, err:%d", err);
    }
#endif

#if CONFIG_ENABLE_CHIP_SHELL
    esp_matter::console::diagnostics_register_commands();
    esp_matter::console::wifi_register_commands();
    esp_matter::console::factoryreset_register_commands();
#if CONFIG_OPENTHREAD_CLI
    esp_matter::console::otcli_register_commands();
#endif
    esp_matter::console::init();
#endif

    ESP_LOGI(TAG, "Matter initialization complete");
}

void MatterAccessControlDriver::ringing() {
    ESP_LOGI(TAG, "Ring detected (occupancy sensor disabled for testing)");
    // TODO: Re-enable when occupancy sensor is back
    // updateOccupancy(true);
}

void MatterAccessControlDriver::unlock() {
    ESP_LOGI(TAG, "Unlock command from Matter");

    updateLockState(true);

    if (onUnlockDispatched_) {
        onUnlockDispatched_();
    }
}

void MatterAccessControlDriver::lock() {
    ESP_LOGI(TAG, "Lock command from Matter (no-op)");

    updateLockState(false);

    if (onLockDispatched_) {
        onLockDispatched_();
    }
}

// ============================================================================
// Private Methods
// ============================================================================

void MatterAccessControlDriver::updateLockState(bool unlocked) {
    if (lockEndpointId_ == 0) {
        ESP_LOGW(TAG, "Lock endpoint not initialized");
        return;
    }

    auto lockState = unlocked ? DoorLock::DlLockState::kUnlocked : DoorLock::DlLockState::kLocked;
    ESP_LOGI(TAG, "Updating Matter lock state to %s", unlocked ? "Unlocked" : "Locked");
    DoorLockServer::Instance().SetLockState(lockEndpointId_, lockState);
}

void MatterAccessControlDriver::updateOccupancy(bool occupied) {
    if (occupancyEndpointId_ == 0) {
        ESP_LOGW(TAG, "Occupancy endpoint not initialized");
        return;
    }

    uint8_t occupancyBitmap = occupied ? 1 : 0;
    ESP_LOGI(TAG, "Updating Matter occupancy to %s", occupied ? "Occupied" : "Unoccupied");

    esp_matter_attr_val_t val = esp_matter_uint8(occupancyBitmap);
    attribute::update(occupancyEndpointId_, OccupancySensing::Id, OccupancySensing::Attributes::Occupancy::Id, &val);
}

void MatterAccessControlDriver::ringTimerCallback(TimerHandle_t timer) {
    ESP_LOGI(TAG, "Ring hold timer expired - clearing occupancy");
    if (auto* self = instance()) {
        self->updateOccupancy(false);
    }
}

// ============================================================================
// Matter SDK Callbacks
// ============================================================================

/**
 * @brief Door Lock cluster initialization callback
 */
__attribute__((used)) void emberAfDoorLockClusterInitCallback(chip::EndpointId endpoint) {
    ESP_LOGI(TAG, "Door Lock cluster init on endpoint %d", endpoint);
    DoorLockServer::Instance().InitServer(endpoint);
}

/**
 * @brief Handle Lock command from Matter
 */
bool emberAfPluginDoorLockOnDoorLockCommand(EndpointId endpointId, const Nullable<FabricIndex>& fabricIdx,
                                            const Nullable<NodeId>& nodeId, const Optional<ByteSpan>& pinCode,
                                            OperationErrorEnum& err) {
    ESP_LOGI(TAG, "Lock command on endpoint %d", endpointId);
    if (auto* driver = MatterAccessControlDriver::instance()) {
        driver->lock();
    }
    return true;
}

/**
 * @brief Handle Unlock command from Matter
 */
bool emberAfPluginDoorLockOnDoorUnlockCommand(EndpointId endpointId, const Nullable<FabricIndex>& fabricIdx,
                                              const Nullable<NodeId>& nodeId, const Optional<ByteSpan>& pinCode,
                                              OperationErrorEnum& err) {
    ESP_LOGI(TAG, "Unlock command on endpoint %d", endpointId);
    if (auto* driver = MatterAccessControlDriver::instance()) {
        driver->unlock();
    }
    return true;
}

/**
 * @brief Called when auto-relock timer fires
 */
void emberAfPluginDoorLockOnAutoRelock(EndpointId endpointId) {
    ESP_LOGI(TAG, "Auto-relock triggered on endpoint %d", endpointId);
    // No hardware action needed - door naturally re-locks
}

// ============================================================================
// File-scope static callbacks
// ============================================================================

static esp_err_t onAttributeUpdate(attribute::callback_type_t type, uint16_t endpoint_id, uint32_t cluster_id,
                                   uint32_t attribute_id, esp_matter_attr_val_t* val, void* priv_data) {
    return ESP_OK;
}

static esp_err_t onIdentification(identification::callback_type_t type, uint16_t endpoint_id, uint8_t effect_id,
                                  uint8_t effect_variant, void* priv_data) {
    ESP_LOGI(TAG, "Identification callback: type: %u, effect: %u, variant: %u", type, effect_id, effect_variant);
    return ESP_OK;
}

static void onDeviceEvent(const chip::DeviceLayer::ChipDeviceEvent* event, intptr_t arg) {
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
