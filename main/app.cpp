/*
   Smart Intercom - Application Implementation
*/

#include "app.h"
#include <esp_log.h>

static const char* TAG = "App";

// ============================================================================
// Public Methods
// ============================================================================

App::App()
    : ringActor_(eventBus_, ringDriver_),
      accessControlActor_(eventBus_, accessControlDriver_),
      doorActor_(eventBus_, doorDriver_) {}

esp_err_t App::init() {
    ESP_LOGI(TAG, "Initializing application...");

    // Initialize access control first (creates Matter endpoints)
    accessControlActor_.init();

    // Initialize door actor (hardware unlock)
    doorActor_.init();

    // Initialize ring actor last (starts listening for hardware events)
    ringActor_.init();

    ESP_LOGI(TAG, "All actors initialized");
    return ESP_OK;
}

// ============================================================================
// Singleton Accessor
// ============================================================================

App& getApp() {
    static App instance;
    return instance;
}
