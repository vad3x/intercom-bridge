/*
   Smart Intercom - GPIO Adapter Implementation

   Hardware interface for:
   - Door unlock relay (PhotoMOS AQY212EHA)
   - Buzz detection via optocoupler (HCPL-817)
*/

#include "hardware/gpio_adapter.h"
#include "config/app_priv.h"
#include <esp_log.h>
#include <driver/gpio.h>
#include <esp_timer.h>
#include <freertos/FreeRTOS.h>
#include <freertos/timers.h>

static const char* TAG = "GpioAdapter";

// Static instance for ISR access
GpioAdapter* GpioAdapter::instance_ = nullptr;

// State tracking
static volatile bool s_buzz_pending = false;
static volatile int64_t s_last_buzz_time = 0;
static TimerHandle_t s_unlock_timer = nullptr;
static TimerHandle_t s_debounce_timer = nullptr;

/**
 * @brief Get current time in milliseconds
 */
static inline int64_t get_time_ms() {
    return esp_timer_get_time() / 1000;
}

/**
 * @brief Debounce timer callback - validates buzz signal
 */
static void debounce_timer_cb(TimerHandle_t timer) {
    if (!s_buzz_pending) {
        return;
    }
    s_buzz_pending = false;

    // Check if signal is still LOW (active) after debounce
    int level = gpio_get_level((gpio_num_t)BUZZ_GPIO);
    if (level != 0) {
        ESP_LOGD(TAG, "Buzz signal bounced");
        return;
    }

    // Check cooldown
    int64_t now = get_time_ms();
    if (s_last_buzz_time > 0 && (now - s_last_buzz_time) < BUZZ_COOLDOWN_MS) {
        ESP_LOGD(TAG, "Buzz ignored (cooldown)");
        return;
    }

    // Valid buzz detected!
    s_last_buzz_time = now;
    ESP_LOGI(TAG, "=== BUZZ/RING DETECTED ===");

    // Call the registered callback
    GpioAdapter::onBuzzDetected();
}

/**
 * @brief GPIO ISR for buzz detection
 */
static void IRAM_ATTR buzz_isr_handler(void* arg) {
    s_buzz_pending = true;

    BaseType_t xHigherPriorityTaskWoken = pdFALSE;
    if (s_debounce_timer) {
        xTimerStartFromISR(s_debounce_timer, &xHigherPriorityTaskWoken);
    }

    if (xHigherPriorityTaskWoken) {
        portYIELD_FROM_ISR();
    }
}

void GpioAdapter::init(Callback onBuzz) {
    ESP_LOGI(TAG, "Initializing GpioAdapter");
    ESP_LOGI(TAG, "  UNLOCK_GPIO: %d (output to PhotoMOS relay)", UNLOCK_GPIO);
    ESP_LOGI(TAG, "  BUZZ_GPIO: %d (input from optocoupler)", BUZZ_GPIO);

    instance_ = this;
    buzzCallback_ = onBuzz;

    // Create unlock pulse timer
    s_unlock_timer = xTimerCreate("unlock",
                                  pdMS_TO_TICKS(UNLOCK_PULSE_MS),
                                  pdFALSE,
                                  this,
                                  [](TimerHandle_t timer) {
                                      GpioAdapter* self = static_cast<GpioAdapter*>(pvTimerGetTimerID(timer));
                                      gpio_set_level((gpio_num_t)UNLOCK_GPIO, 0);
                                      self->onUnlockComplete();
                                  });

    if (!s_unlock_timer) {
        ESP_LOGE(TAG, "Failed to create unlock timer");
    }

    // Create debounce timer
    s_debounce_timer = xTimerCreate("debounce",
                                    pdMS_TO_TICKS(DEBOUNCE_MS),
                                    pdFALSE,
                                    nullptr,
                                    debounce_timer_cb);

    if (!s_debounce_timer) {
        ESP_LOGE(TAG, "Failed to create debounce timer");
    }

    // Configure UNLOCK_GPIO as output
    gpio_config_t unlock_conf = {
        .pin_bit_mask = (1ULL << UNLOCK_GPIO),
        .mode = GPIO_MODE_OUTPUT,
        .pull_up_en = GPIO_PULLUP_DISABLE,
        .pull_down_en = GPIO_PULLDOWN_DISABLE,
        .intr_type = GPIO_INTR_DISABLE,
    };
    esp_err_t ret = gpio_config(&unlock_conf);
    if (ret != ESP_OK) {
        ESP_LOGE(TAG, "Failed to configure UNLOCK_GPIO: %s", esp_err_to_name(ret));
    }
    gpio_set_level((gpio_num_t)UNLOCK_GPIO, 0);  // Start LOW (relay off)

    // Configure BUZZ_GPIO as input with pull-up
    gpio_config_t buzz_conf = {
        .pin_bit_mask = (1ULL << BUZZ_GPIO),
        .mode = GPIO_MODE_INPUT,
        .pull_up_en = GPIO_PULLUP_ENABLE,
        .pull_down_en = GPIO_PULLDOWN_DISABLE,
        .intr_type = GPIO_INTR_NEGEDGE,  // Falling edge (opto pulls LOW)
    };
    ret = gpio_config(&buzz_conf);
    if (ret != ESP_OK) {
        ESP_LOGE(TAG, "Failed to configure BUZZ_GPIO: %s", esp_err_to_name(ret));
    }

    // Install GPIO ISR service
    ret = gpio_install_isr_service(0);
    if (ret != ESP_OK && ret != ESP_ERR_INVALID_STATE) {
        ESP_LOGE(TAG, "Failed to install GPIO ISR service: %s", esp_err_to_name(ret));
    }

    // Add ISR handler for buzz detection
    ret = gpio_isr_handler_add((gpio_num_t)BUZZ_GPIO, buzz_isr_handler, nullptr);
    if (ret != ESP_OK) {
        ESP_LOGE(TAG, "Failed to add buzz ISR handler: %s", esp_err_to_name(ret));
    }

    ESP_LOGI(TAG, "GPIO initialization complete");
}

void GpioAdapter::triggerUnlock() {
    if (unlockInProgress_) {
        ESP_LOGW(TAG, "Unlock already in progress");
        return;
    }

    ESP_LOGI(TAG, "Starting unlock pulse (%d ms)", UNLOCK_PULSE_MS);

    unlockInProgress_ = true;
    gpio_set_level((gpio_num_t)UNLOCK_GPIO, 1);  // Activate relay

    // Start timer to end pulse
    if (xTimerStart(s_unlock_timer, pdMS_TO_TICKS(100)) != pdPASS) {
        gpio_set_level((gpio_num_t)UNLOCK_GPIO, 0);
        unlockInProgress_ = false;
        ESP_LOGE(TAG, "Failed to start unlock timer");
    }
}

bool GpioAdapter::isUnlocking() const {
    return unlockInProgress_;
}

void GpioAdapter::onBuzzDetected() {
    if (instance_ && instance_->buzzCallback_) {
        instance_->buzzCallback_();
    }
}

void GpioAdapter::onUnlockComplete() {
    unlockInProgress_ = false;
    ESP_LOGI(TAG, "Unlock pulse ended");
}

