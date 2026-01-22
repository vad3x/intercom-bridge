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

// Matter setup code generation
#include <setup_payload/QRCodeSetupPayloadGenerator.h>
#include <setup_payload/ManualSetupPayloadGenerator.h>
#include <setup_payload/SetupPayload.h>
#include <cstring>
#include <inttypes.h>

static const char* TAG = "MatterDriver";

// Endpoint IDs
static constexpr uint16_t ENDPOINT_ID_OCCUPANCY_SENSOR = 2;

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
    strcpy(node_config.root_node.basic_information.node_label, "Smart Intercom");
    s_node = node::create(&node_config, onAttributeUpdate, onIdentification);
    if (!s_node) {
        ESP_LOGE(TAG, "Failed to create Matter node");
        return;
    }
    ESP_LOGI(TAG, "Matter node created");

    // Create Door Lock endpoint
    door_lock::config_t door_lock_config;
    // Set initial state to locked (door is always locked by spring mechanism)
    door_lock_config.door_lock.lock_state = static_cast<uint8_t>(DoorLock::DlLockState::kLocked);
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

    // Create Occupancy Sensor endpoint (appears as Motion Sensor in HomeKit)
    occupancy_sensor::config_t occupancy_config;
    // Set feature flags to PIR (HomeKit expects this for motion)
    occupancy_config.occupancy_sensing.feature_flags = 0x01; // kPir
    occupancy_config.occupancy_sensing.occupancy = 0;        // Initially unoccupied
    
    endpoint_t* occupancy_endpoint =
        occupancy_sensor::create(s_node, &occupancy_config, ENDPOINT_FLAG_NONE, nullptr);
    if (!occupancy_endpoint) {
        ESP_LOGE(TAG, "Failed to create occupancy sensor endpoint");
        return;
    }

    occupancyEndpointId_ = endpoint::get_id(occupancy_endpoint);
    ESP_LOGI(TAG, "Occupancy sensor created with endpoint_id %d", occupancyEndpointId_);

    // Start Matter stack
    ESP_LOGI(TAG, "Starting Matter stack");
    esp_err_t err = esp_matter::start(onDeviceEvent);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "Failed to start Matter, err:%d", err);
        return;
    }

    // Update Basic Information strings (Home uses these for the default name)
    static char kProductName[] = "Smart Intercom";
    static char kVendorName[] = "Smart Intercom";
    static char kNodeLabel[] = "Smart Intercom";
    esp_matter_attr_val_t product_name = esp_matter_char_str(kProductName, strlen(kProductName));
    esp_matter_attr_val_t vendor_name = esp_matter_char_str(kVendorName, strlen(kVendorName));
    esp_matter_attr_val_t node_label = esp_matter_char_str(kNodeLabel, strlen(kNodeLabel));
    err = attribute::update(0, BasicInformation::Id,
                            BasicInformation::Attributes::ProductName::Id, &product_name);
    if (err != ESP_OK) {
        ESP_LOGW(TAG, "Failed to set product name, err:%d", err);
    }
    err = attribute::update(0, BasicInformation::Id,
                            BasicInformation::Attributes::VendorName::Id, &vendor_name);
    if (err != ESP_OK) {
        ESP_LOGW(TAG, "Failed to set vendor name, err:%d", err);
    }
    err = attribute::update(0, BasicInformation::Id,
                            BasicInformation::Attributes::NodeLabel::Id, &node_label);
    if (err != ESP_OK) {
        ESP_LOGW(TAG, "Failed to set node label, err:%d", err);
    }

    // Set initial occupancy state to Unoccupied AFTER Matter stack is running
    ESP_LOGI(TAG, "Setting initial occupancy state to Unoccupied");
    esp_matter_attr_val_t initial_state = esp_matter_bitmap8(0); // 0 = Unoccupied
    err = attribute::update(occupancyEndpointId_, OccupancySensing::Id, 
                            OccupancySensing::Attributes::Occupancy::Id, &initial_state);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "Failed to set initial occupancy state, err:%d", err);
    } else {
        ESP_LOGI(TAG, "Occupancy sensor initial state set successfully");
    }

    // Log setup codes for easy commissioning
    logSetupCodes();

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
    ESP_LOGI(TAG, "Ring detected - activating occupancy sensor");

    updateOccupancy(true);

    // Reset/start timer for auto-clear
    if (ringTimer_) {
        xTimerStop(ringTimer_, 0);
        xTimerChangePeriod(ringTimer_, pdMS_TO_TICKS(config_.ringHoldMs), 0);
        xTimerStart(ringTimer_, 0);
    }
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
        ESP_LOGW(TAG, "Occupancy sensor endpoint not initialized");
        return;
    }

    ESP_LOGI(TAG, "Updating Matter occupancy to %s", occupied ? "Occupied" : "Unoccupied");

    esp_matter_attr_val_t val = esp_matter_bitmap8(occupied ? 1 : 0);
    esp_err_t err = attribute::update(occupancyEndpointId_, OccupancySensing::Id, 
                                       OccupancySensing::Attributes::Occupancy::Id, &val);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "Failed to update occupancy attribute: %d", err);
    }
}

void MatterAccessControlDriver::ringTimerCallback(TimerHandle_t timer) {
    ESP_LOGI(TAG, "Ring hold timer expired - clearing occupancy");
    if (auto* self = instance()) {
        self->updateOccupancy(false);
    }
}

void MatterAccessControlDriver::logSetupCodes() {
    // Get default values from configuration
    // These should match the values in sdkconfig.defaults
    uint16_t discriminator = 3840;     // CONFIG_DISCRIMINATOR
    uint32_t setupPinCode = 20202021;  // CONFIG_SETUP_PIN_CODE

    chip::SetupDiscriminator setupDisc;
    setupDisc.SetLongValue(discriminator);
    chip::SetupPayload payload;
    payload.version = 0;
    payload.vendorID = 0xFFF1;   // CONFIG_DEVICE_VENDOR_ID
    payload.productID = 0x8001;  // CONFIG_DEVICE_PRODUCT_ID
    payload.discriminator = setupDisc;
    payload.setUpPINCode = setupPinCode;
    payload.commissioningFlow = chip::CommissioningFlow::kStandard;
    
    // Set rendezvous information to BLE
    chip::BitFlags<chip::RendezvousInformationFlag, uint8_t> rendezvousFlags;
    rendezvousFlags.Set(chip::RendezvousInformationFlag::kBLE);
    payload.rendezvousInformation.SetValue(rendezvousFlags);

    // Generate QR code (Base38 format)
    std::string qrCode;
    chip::QRCodeSetupPayloadGenerator qrGenerator(payload);
    CHIP_ERROR err = qrGenerator.payloadBase38Representation(qrCode);
    if (err != CHIP_NO_ERROR) {
        ESP_LOGE(TAG, "Failed to generate QR code: %" CHIP_ERROR_FORMAT, err.Format());
        return;
    }

    // Generate manual pairing code (Short decimal format)
    std::string manualCode;
    chip::ManualSetupPayloadGenerator manualGenerator(payload);
    CHIP_ERROR manualErr = manualGenerator.payloadDecimalStringRepresentation(manualCode);
    if (manualErr != CHIP_NO_ERROR) {
        ESP_LOGE(TAG, "Failed to generate manual code: %" CHIP_ERROR_FORMAT, manualErr.Format());
        return;
    }

    // Log the codes
    ESP_LOGI(TAG, "==================================================");
    ESP_LOGI(TAG, "🎉 COMMISSIONING CODES - Ready to Add Device!");
    ESP_LOGI(TAG, "==================================================");
    ESP_LOGI(TAG, "📱 Manual Setup Code: %s", manualCode.c_str());
    ESP_LOGI(TAG, "📷 QR Code URL: https://project-chip.github.io/connectedhomeip/qrcode.html?data=%s", qrCode.c_str());
    ESP_LOGI(TAG, "🔢 Setup PIN Code: %" PRIu32, setupPinCode);
    ESP_LOGI(TAG, "🎯 Discriminator: %u", discriminator);
    ESP_LOGI(TAG, "==================================================");
    ESP_LOGI(TAG, "💡 Add this device to your Matter controller app!");
    ESP_LOGI(TAG, "==================================================");
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
    
    // Force initial state to Locked (door is mechanically locked by spring)
    ESP_LOGI(TAG, "Setting initial lock state to Locked on endpoint %d", endpoint);
    DoorLockServer::Instance().SetLockState(endpoint, DoorLock::DlLockState::kLocked);
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
