/*
   Smart Intercom - Application Implementation
*/

#include "app.h"
#include "config/app_config.h"
#include <esp_log.h>

static const char* TAG = "App";

// ============================================================================
// Public Methods
// ============================================================================

App::App()
#if LED_ENABLED
    : indicatorDriver_({.gpioPin = LED_GPIO, .brightness = LED_BRIGHTNESS}),
#else
    : indicatorDriver_(),
#endif
      ringDriver_({.gpioPin = RING_GPIO, .debounceMs = RING_DEBOUNCE_MS, .cooldownMs = RING_COOLDOWN_MS}),
      doorDriver_({.gpioPin = UNLOCK_GPIO, .unlockPulseMs = UNLOCK_PULSE_MS}),
      accessControlDriver_({.ringHoldMs = RING_HOLD_MS, .autoRelockSec = AUTO_RELOCK_SEC}),
      indicatorActor_(eventBus_, indicatorDriver_),
      ringActor_(eventBus_, ringDriver_),
      accessControlActor_(eventBus_, accessControlDriver_),
      doorActor_(eventBus_, doorDriver_) {}

esp_err_t App::init() {
    ESP_LOGI(TAG, "Initializing application...");

    // Initialize indicator actor first (subscribes to events)
    indicatorActor_.init();

    // Signal: initializing
    eventBus_.emitSystemState(TAG, SystemState::Initializing);

    // Initialize access control (creates Matter endpoints)
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
