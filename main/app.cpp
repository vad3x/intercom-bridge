/*
   Smart Intercom - Application Implementation
*/

#include "app.h"
#include "config/app_priv.h"
#include <esp_log.h>

static const char* TAG = "App";

App::App()
    : ringActor_(eventBus_, gpioAdapter_)
    , intercomActor_(eventBus_)
    , lockActor_(eventBus_, gpioAdapter_)
    , matterActor_(eventBus_) {
}

esp_err_t App::init() {
    ESP_LOGI(TAG, "Initializing application...");

    esp_err_t err = matterActor_.init();
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "Failed to initialize MatterActor");
        return err;
    }

    intercomActor_.init(BUZZ_HOLD_MS);
    lockActor_.init();
    ringActor_.init();

    ESP_LOGI(TAG, "All actors initialized");
    return ESP_OK;
}

