/*
   Smart Intercom - GPIO Driver
   
   Hardware interface for:
   - Door unlock relay (PhotoMOS AQY212EHA)
   - Buzz detection via optocoupler (HCPL-817)
*/

#include <esp_log.h>
#include <driver/gpio.h>
#include <esp_timer.h>
#include <freertos/FreeRTOS.h>
#include <freertos/task.h>
#include <freertos/timers.h>

#include <esp_matter.h>
#include <app_priv.h>

static const char *TAG = "app_driver";

using namespace esp_matter;
using namespace chip::app::Clusters;

// State tracking
static volatile bool s_unlock_in_progress = false;
static volatile bool s_buzz_pending = false;
static volatile int64_t s_last_buzz_time = 0;
static TimerHandle_t s_unlock_timer = NULL;
static TimerHandle_t s_debounce_timer = NULL;
static TimerHandle_t s_buzz_release_timer = NULL;

/**
 * @brief Get current time in milliseconds
 */
static inline int64_t get_time_ms()
{
    return esp_timer_get_time() / 1000;
}

/**
 * @brief Timer callback to end unlock pulse
 */
static void unlock_timer_cb(TimerHandle_t timer)
{
    gpio_set_level((gpio_num_t)UNLOCK_GPIO, 0);
    s_unlock_in_progress = false;
    ESP_LOGI(TAG, "Unlock pulse ended");
}

/**
 * @brief Release button after hold time
 */
static void buzz_release_timer_cb(TimerHandle_t timer)
{
    ESP_LOGI(TAG, "Buzz button released (auto-reset after %d ms)", BUZZ_HOLD_MS);
    
    // Release button (position 0 = released)
    esp_matter_attr_val_t val = esp_matter_uint8(0);
    attribute::update(buzz_button_endpoint_id, 
                      Switch::Id, 
                      Switch::Attributes::CurrentPosition::Id, 
                      &val);
}

/**
 * @brief Send button press event to Matter (GenericSwitch cluster)
 */
static void send_buzz_button_press()
{
    if (buzz_button_endpoint_id == 0) {
        ESP_LOGW(TAG, "Buzz button endpoint not initialized");
        return;
    }
    
    ESP_LOGI(TAG, "Sending buzz button press event (will hold for %d ms)", BUZZ_HOLD_MS);
    
    // Press button (position 1 = pressed)
    esp_matter_attr_val_t val = esp_matter_uint8(1);
    attribute::update(buzz_button_endpoint_id, 
                      Switch::Id, 
                      Switch::Attributes::CurrentPosition::Id, 
                      &val);
    
    // Start timer to auto-release after BUZZ_HOLD_MS
    if (s_buzz_release_timer) {
        xTimerStart(s_buzz_release_timer, 0);
    }
}

/**
 * @brief Debounce timer callback - validates buzz signal
 */
static void debounce_timer_cb(TimerHandle_t timer)
{
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
    
    // Send button press event
    send_buzz_button_press();
}

/**
 * @brief GPIO ISR for buzz detection
 */
static void IRAM_ATTR buzz_isr_handler(void *arg)
{
    s_buzz_pending = true;
    
    BaseType_t xHigherPriorityTaskWoken = pdFALSE;
    if (s_debounce_timer) {
        xTimerStartFromISR(s_debounce_timer, &xHigherPriorityTaskWoken);
    }
    
    if (xHigherPriorityTaskWoken) {
        portYIELD_FROM_ISR();
    }
}

esp_err_t app_driver_gpio_init()
{
    ESP_LOGI(TAG, "Initializing GPIO for intercom hardware");
    ESP_LOGI(TAG, "  UNLOCK_GPIO: %d (output to PhotoMOS relay)", UNLOCK_GPIO);
    ESP_LOGI(TAG, "  BUZZ_GPIO: %d (input from optocoupler)", BUZZ_GPIO);
    
    // Create unlock pulse timer
    s_unlock_timer = xTimerCreate("unlock", pdMS_TO_TICKS(UNLOCK_PULSE_MS), 
                                   pdFALSE, NULL, unlock_timer_cb);
    if (!s_unlock_timer) {
        ESP_LOGE(TAG, "Failed to create unlock timer");
        return ESP_ERR_NO_MEM;
    }
    
    // Create debounce timer
    s_debounce_timer = xTimerCreate("debounce", pdMS_TO_TICKS(DEBOUNCE_MS),
                                     pdFALSE, NULL, debounce_timer_cb);
    if (!s_debounce_timer) {
        ESP_LOGE(TAG, "Failed to create debounce timer");
        return ESP_ERR_NO_MEM;
    }
    
    // Create buzz release timer (auto-releases button after BUZZ_HOLD_MS)
    s_buzz_release_timer = xTimerCreate("buzz_release", pdMS_TO_TICKS(BUZZ_HOLD_MS),
                                         pdFALSE, NULL, buzz_release_timer_cb);
    if (!s_buzz_release_timer) {
        ESP_LOGE(TAG, "Failed to create buzz release timer");
        return ESP_ERR_NO_MEM;
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
        return ret;
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
        return ret;
    }
    
    // Install GPIO ISR service
    ret = gpio_install_isr_service(0);
    if (ret != ESP_OK && ret != ESP_ERR_INVALID_STATE) {
        ESP_LOGE(TAG, "Failed to install GPIO ISR service: %s", esp_err_to_name(ret));
        return ret;
    }
    
    // Add ISR handler for buzz detection
    ret = gpio_isr_handler_add((gpio_num_t)BUZZ_GPIO, buzz_isr_handler, NULL);
    if (ret != ESP_OK) {
        ESP_LOGE(TAG, "Failed to add buzz ISR handler: %s", esp_err_to_name(ret));
        return ret;
    }
    
    ESP_LOGI(TAG, "GPIO initialization complete");
    return ESP_OK;
}

esp_err_t app_driver_unlock_pulse()
{
    if (s_unlock_in_progress) {
        ESP_LOGW(TAG, "Unlock already in progress");
        return ESP_ERR_INVALID_STATE;
    }
    
    ESP_LOGI(TAG, "Starting unlock pulse (%d ms)", UNLOCK_PULSE_MS);
    
    s_unlock_in_progress = true;
    gpio_set_level((gpio_num_t)UNLOCK_GPIO, 1);  // Activate relay
    
    // Start timer to end pulse
    if (xTimerStart(s_unlock_timer, pdMS_TO_TICKS(100)) != pdPASS) {
        gpio_set_level((gpio_num_t)UNLOCK_GPIO, 0);
        s_unlock_in_progress = false;
        ESP_LOGE(TAG, "Failed to start unlock timer");
        return ESP_FAIL;
    }
    
    return ESP_OK;
}

bool app_driver_is_unlocking()
{
    return s_unlock_in_progress;
}

esp_err_t app_driver_attribute_update(app_driver_handle_t driver_handle, uint16_t endpoint_id, 
                                      uint32_t cluster_id, uint32_t attribute_id, 
                                      esp_matter_attr_val_t *val)
{
    // Handle attribute updates if needed
    // Currently, all hardware control is done through the lock manager
    return ESP_OK;
}

app_driver_handle_t app_driver_button_init()
{
    // Button initialization removed - using custom GPIO for intercom
    // Return NULL as we don't use the BSP button interface
    ESP_LOGI(TAG, "Button init skipped (using custom GPIO for intercom)");
    return NULL;
}
