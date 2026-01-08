/**
 * @file gpio_handler.c
 * @brief GPIO handling for Smart Intercom (buzz detection + door unlock)
 */

#include "gpio_handler.h"
#include "driver/gpio.h"
#include "esp_timer.h"
#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "freertos/semphr.h"
#include "freertos/timers.h"

static const char *TAG = "gpio_handler";

// State variables
static volatile int64_t s_last_buzz_time = 0;
static volatile int64_t s_last_unlock_time = 0;
static volatile uint32_t s_buzz_count = 0;
static volatile uint32_t s_unlock_count = 0;
static volatile bool s_unlock_in_progress = false;

// Synchronization
static SemaphoreHandle_t s_unlock_mutex = NULL;
static TimerHandle_t s_debounce_timer = NULL;
static TimerHandle_t s_unlock_timer = NULL;

// Callback
static buzz_callback_t s_buzz_callback = NULL;

// Debounce state
static volatile bool s_pending_buzz = false;
static volatile int64_t s_isr_time = 0;

/**
 * @brief Get current time in milliseconds
 */
static inline int64_t get_time_ms(void)
{
    return esp_timer_get_time() / 1000;
}

/**
 * @brief GPIO ISR handler for buzz detection
 */
static void IRAM_ATTR buzz_isr_handler(void *arg)
{
    // Record ISR time and flag for debounce processing
    s_isr_time = esp_timer_get_time() / 1000;
    s_pending_buzz = true;
    
    // Start debounce timer from ISR
    BaseType_t xHigherPriorityTaskWoken = pdFALSE;
    xTimerStartFromISR(s_debounce_timer, &xHigherPriorityTaskWoken);
    
    if (xHigherPriorityTaskWoken) {
        portYIELD_FROM_ISR();
    }
}

/**
 * @brief Debounce timer callback
 * Called after debounce period to validate buzz signal
 */
static void debounce_timer_callback(TimerHandle_t xTimer)
{
    if (!s_pending_buzz) {
        return;
    }
    s_pending_buzz = false;
    
    // Check if signal is still LOW (active) after debounce
    int level = gpio_get_level(BUZZ_GPIO);
    if (level != 0) {
        ESP_LOGD(TAG, "Buzz signal bounced (now HIGH)");
        return;
    }
    
    // Check cooldown
    int64_t now = get_time_ms();
    if (s_last_buzz_time > 0 && (now - s_last_buzz_time) < BUZZ_COOLDOWN_MS) {
        ESP_LOGD(TAG, "Buzz ignored (cooldown active)");
        return;
    }
    
    // Valid buzz event
    s_last_buzz_time = s_isr_time;
    s_buzz_count++;
    
    ESP_LOGI(TAG, "Buzz detected! Count: %lu, Time: %lld ms", 
             (unsigned long)s_buzz_count, s_last_buzz_time);
    
    // Invoke callback if registered
    if (s_buzz_callback) {
        s_buzz_callback(s_last_buzz_time);
    }
}

/**
 * @brief Unlock pulse timer callback
 * Called to end the unlock pulse
 */
static void unlock_timer_callback(TimerHandle_t xTimer)
{
    // End the unlock pulse
    gpio_set_level(UNLOCK_GPIO, 0);
    s_unlock_in_progress = false;
    
    ESP_LOGI(TAG, "Unlock pulse ended");
}

esp_err_t gpio_handler_init(buzz_callback_t callback)
{
    ESP_LOGI(TAG, "Initializing GPIO handler");
    ESP_LOGI(TAG, "  BUZZ_GPIO: %d (input, pull-up)", BUZZ_GPIO);
    ESP_LOGI(TAG, "  UNLOCK_GPIO: %d (output)", UNLOCK_GPIO);
    
    s_buzz_callback = callback;
    
    // Create mutex for unlock operation
    s_unlock_mutex = xSemaphoreCreateMutex();
    if (s_unlock_mutex == NULL) {
        ESP_LOGE(TAG, "Failed to create unlock mutex");
        return ESP_ERR_NO_MEM;
    }
    
    // Create debounce timer (one-shot)
    s_debounce_timer = xTimerCreate(
        "debounce",
        pdMS_TO_TICKS(DEBOUNCE_TIME_MS),
        pdFALSE,  // One-shot
        NULL,
        debounce_timer_callback
    );
    if (s_debounce_timer == NULL) {
        ESP_LOGE(TAG, "Failed to create debounce timer");
        vSemaphoreDelete(s_unlock_mutex);
        return ESP_ERR_NO_MEM;
    }
    
    // Create unlock pulse timer (one-shot)
    s_unlock_timer = xTimerCreate(
        "unlock",
        pdMS_TO_TICKS(UNLOCK_PULSE_MS),
        pdFALSE,  // One-shot
        NULL,
        unlock_timer_callback
    );
    if (s_unlock_timer == NULL) {
        ESP_LOGE(TAG, "Failed to create unlock timer");
        xTimerDelete(s_debounce_timer, 0);
        vSemaphoreDelete(s_unlock_mutex);
        return ESP_ERR_NO_MEM;
    }
    
    // Configure BUZZ_GPIO as input with pull-up
    gpio_config_t buzz_conf = {
        .pin_bit_mask = (1ULL << BUZZ_GPIO),
        .mode = GPIO_MODE_INPUT,
        .pull_up_en = GPIO_PULLUP_ENABLE,
        .pull_down_en = GPIO_PULLDOWN_DISABLE,
        .intr_type = GPIO_INTR_NEGEDGE,  // Falling edge (opto pulls LOW)
    };
    esp_err_t ret = gpio_config(&buzz_conf);
    if (ret != ESP_OK) {
        ESP_LOGE(TAG, "Failed to configure BUZZ_GPIO: %s", esp_err_to_name(ret));
        return ret;
    }
    
    // Configure UNLOCK_GPIO as output
    gpio_config_t unlock_conf = {
        .pin_bit_mask = (1ULL << UNLOCK_GPIO),
        .mode = GPIO_MODE_OUTPUT,
        .pull_up_en = GPIO_PULLUP_DISABLE,
        .pull_down_en = GPIO_PULLDOWN_DISABLE,
        .intr_type = GPIO_INTR_DISABLE,
    };
    ret = gpio_config(&unlock_conf);
    if (ret != ESP_OK) {
        ESP_LOGE(TAG, "Failed to configure UNLOCK_GPIO: %s", esp_err_to_name(ret));
        return ret;
    }
    
    // Ensure unlock GPIO starts LOW
    gpio_set_level(UNLOCK_GPIO, 0);
    
    // Install GPIO ISR service and add handler
    ret = gpio_install_isr_service(0);
    if (ret != ESP_OK && ret != ESP_ERR_INVALID_STATE) {
        // ESP_ERR_INVALID_STATE means service already installed, which is OK
        ESP_LOGE(TAG, "Failed to install GPIO ISR service: %s", esp_err_to_name(ret));
        return ret;
    }
    
    ret = gpio_isr_handler_add(BUZZ_GPIO, buzz_isr_handler, NULL);
    if (ret != ESP_OK) {
        ESP_LOGE(TAG, "Failed to add ISR handler: %s", esp_err_to_name(ret));
        return ret;
    }
    
    ESP_LOGI(TAG, "GPIO handler initialized successfully");
    return ESP_OK;
}

esp_err_t gpio_unlock_door(void)
{
    // Try to acquire mutex with timeout
    if (xSemaphoreTake(s_unlock_mutex, pdMS_TO_TICKS(100)) != pdTRUE) {
        ESP_LOGW(TAG, "Unlock mutex acquisition timeout");
        return ESP_ERR_TIMEOUT;
    }
    
    // Check if unlock is in progress
    if (s_unlock_in_progress) {
        xSemaphoreGive(s_unlock_mutex);
        ESP_LOGW(TAG, "Unlock already in progress");
        return ESP_ERR_INVALID_STATE;
    }
    
    // Check lockout period
    int64_t now = get_time_ms();
    if (s_last_unlock_time > 0 && (now - s_last_unlock_time) < UNLOCK_LOCKOUT_MS) {
        xSemaphoreGive(s_unlock_mutex);
        ESP_LOGW(TAG, "Unlock lockout active (wait %lld ms)", 
                 UNLOCK_LOCKOUT_MS - (now - s_last_unlock_time));
        return ESP_ERR_INVALID_STATE;
    }
    
    // Start unlock pulse
    s_unlock_in_progress = true;
    s_last_unlock_time = now;
    s_unlock_count++;
    
    ESP_LOGI(TAG, "Starting unlock pulse (%d ms). Count: %lu", 
             UNLOCK_PULSE_MS, (unsigned long)s_unlock_count);
    
    gpio_set_level(UNLOCK_GPIO, 1);
    
    // Start timer to end the pulse
    if (xTimerStart(s_unlock_timer, pdMS_TO_TICKS(100)) != pdPASS) {
        // Timer failed to start, end pulse immediately
        gpio_set_level(UNLOCK_GPIO, 0);
        s_unlock_in_progress = false;
        xSemaphoreGive(s_unlock_mutex);
        ESP_LOGE(TAG, "Failed to start unlock timer");
        return ESP_FAIL;
    }
    
    xSemaphoreGive(s_unlock_mutex);
    return ESP_OK;
}

bool gpio_is_unlock_busy(void)
{
    if (s_unlock_in_progress) {
        return true;
    }
    
    int64_t now = get_time_ms();
    if (s_last_unlock_time > 0 && (now - s_last_unlock_time) < UNLOCK_LOCKOUT_MS) {
        return true;
    }
    
    return false;
}

int64_t gpio_get_last_buzz_time(void)
{
    return s_last_buzz_time;
}

int64_t gpio_get_last_unlock_time(void)
{
    return s_last_unlock_time;
}

uint32_t gpio_get_buzz_count(void)
{
    return s_buzz_count;
}

uint32_t gpio_get_unlock_count(void)
{
    return s_unlock_count;
}

void gpio_handler_deinit(void)
{
    ESP_LOGI(TAG, "Deinitializing GPIO handler");
    
    // Remove ISR handler
    gpio_isr_handler_remove(BUZZ_GPIO);
    
    // Delete timers
    if (s_debounce_timer) {
        xTimerDelete(s_debounce_timer, portMAX_DELAY);
        s_debounce_timer = NULL;
    }
    if (s_unlock_timer) {
        xTimerDelete(s_unlock_timer, portMAX_DELAY);
        s_unlock_timer = NULL;
    }
    
    // Delete mutex
    if (s_unlock_mutex) {
        vSemaphoreDelete(s_unlock_mutex);
        s_unlock_mutex = NULL;
    }
    
    // Reset GPIO to safe state
    gpio_set_level(UNLOCK_GPIO, 0);
    
    // Reset state
    s_buzz_callback = NULL;
    s_last_buzz_time = 0;
    s_last_unlock_time = 0;
    s_buzz_count = 0;
    s_unlock_count = 0;
    s_unlock_in_progress = false;
    
    ESP_LOGI(TAG, "GPIO handler deinitialized");
}
