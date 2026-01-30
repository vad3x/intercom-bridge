/*
   Smart Intercom - GPIO Ring Driver Implementation

   Hardware interface for ring detection via optocoupler (HCPL-817).
   Uses a dedicated task to handle ring callbacks (avoids timer stack overflow).
*/

#include "drivers/gpio_ring_driver.h"
#include <driver/gpio.h>
#include <esp_log.h>
#include <esp_timer.h>

static const char* TAG = "GpioRingDriver";

// Stack size for ring handler task (needs enough for Matter attribute updates)
#define RING_TASK_STACK_SIZE 4096
#define RING_TASK_PRIORITY 5

// ============================================================================
// ISR Handler (must be file-scope for IRAM_ATTR)
// ============================================================================

static void IRAM_ATTR ring_isr_handler(void* arg) {
    auto* self = static_cast<GpioRingDriver*>(arg);
    self->ringPending_ = true;

    BaseType_t xHigherPriorityTaskWoken = pdFALSE;
    if (self->debounceTimer_) {
        xTimerStartFromISR(self->debounceTimer_, &xHigherPriorityTaskWoken);
    }

    if (xHigherPriorityTaskWoken) {
        portYIELD_FROM_ISR();
    }
}

// ============================================================================
// Ring Handler Task
// ============================================================================

static void ring_handler_task(void* arg) {
    auto* self = static_cast<GpioRingDriver*>(arg);

    while (true) {
        // Wait for notification from debounce timer
        ulTaskNotifyTake(pdTRUE, portMAX_DELAY);

        if (self->ringValidated_) {
            self->ringValidated_ = false;

            ESP_LOGI(TAG, "=== RING DETECTED ===");

            // Call the ring callback (Matter update happens here)
            // LED feedback is handled by LedActor observing the RingingEvent
            if (self->ringCallback_) {
                self->ringCallback_();
            }
        }
    }
}

// ============================================================================
// Debounce Timer Callback
// ============================================================================

static void debounce_timer_callback(TimerHandle_t timer) {
    auto* self = static_cast<GpioRingDriver*>(pvTimerGetTimerID(timer));

    if (!self->ringPending_) {
        return;
    }
    self->ringPending_ = false;

    // Check if signal is still LOW (active) after debounce
    int level = gpio_get_level((gpio_num_t)self->config_.gpioPin);
    if (level != 0) {
        ESP_LOGD(TAG, "Ring signal bounced");
        return;
    }

    // Check cooldown
    int64_t now = esp_timer_get_time() / 1000;
    if (self->lastRingTime_ > 0 && (now - self->lastRingTime_) < self->config_.cooldownMs) {
        ESP_LOGD(TAG, "Ring ignored (cooldown)");
        return;
    }

    // Valid ring - update timestamp and notify handler task
    self->lastRingTime_ = now;
    self->ringValidated_ = true;

    // Notify the ring handler task (from timer context is safe)
    BaseType_t xHigherPriorityTaskWoken = pdFALSE;
    vTaskNotifyGiveFromISR(self->ringTask_, &xHigherPriorityTaskWoken);
    portYIELD_FROM_ISR(xHigherPriorityTaskWoken);
}

// ============================================================================
// Public Methods
// ============================================================================

void GpioRingDriver::init(OnRingCallback onRing) {
    ESP_LOGI(TAG, "Initializing GpioRingDriver");
    ESP_LOGI(TAG, "  GPIO pin: %d (input from optocoupler)", config_.gpioPin);
    ESP_LOGI(TAG, "  Debounce: %lu ms", (unsigned long)config_.debounceMs);
    ESP_LOGI(TAG, "  Cooldown: %lu ms", (unsigned long)config_.cooldownMs);

    ringCallback_ = onRing;

    // Create ring handler task with adequate stack for Matter operations
    BaseType_t ret =
        xTaskCreate(ring_handler_task, "ring_handler", RING_TASK_STACK_SIZE, this, RING_TASK_PRIORITY, &ringTask_);
    if (ret != pdPASS) {
        ESP_LOGE(TAG, "Failed to create ring handler task");
    }

    // Create debounce timer (pass 'this' via timer ID)
    debounceTimer_ =
        xTimerCreate("debounce", pdMS_TO_TICKS(config_.debounceMs), pdFALSE, this, debounce_timer_callback);
    if (!debounceTimer_) {
        ESP_LOGE(TAG, "Failed to create debounce timer");
    }

    // Configure GPIO as input with pull-up
    gpio_config_t ring_conf = {
        .pin_bit_mask = (1ULL << config_.gpioPin),
        .mode = GPIO_MODE_INPUT,
        .pull_up_en = GPIO_PULLUP_ENABLE,
        .pull_down_en = GPIO_PULLDOWN_DISABLE,
        .intr_type = GPIO_INTR_NEGEDGE,  // Falling edge (opto pulls LOW)
    };
    esp_err_t err = gpio_config(&ring_conf);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "Failed to configure GPIO: %s", esp_err_to_name(err));
    }

    // Install GPIO ISR service
    err = gpio_install_isr_service(0);
    if (err != ESP_OK && err != ESP_ERR_INVALID_STATE) {
        ESP_LOGE(TAG, "Failed to install GPIO ISR service: %s", esp_err_to_name(err));
    }

    // Add ISR handler for ring detection (pass 'this' via arg)
    err = gpio_isr_handler_add((gpio_num_t)config_.gpioPin, ring_isr_handler, this);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "Failed to add ISR handler: %s", esp_err_to_name(err));
    }

    ESP_LOGI(TAG, "Ring detection initialized (with dedicated handler task)");
}
