/**
 * @file gpio_handler.h
 * @brief GPIO handling for Smart Intercom (buzz detection + door unlock)
 * 
 * Hardware interface:
 * - BUZZ_GPIO: Input from optocoupler (HCPL-817) detecting ring signal
 * - UNLOCK_GPIO: Output to PhotoMOS relay (AQY212EHA) for door unlock
 */

#ifndef GPIO_HANDLER_H
#define GPIO_HANDLER_H

#include <stdint.h>
#include <stdbool.h>
#include "esp_err.h"

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @brief GPIO pin configuration
 * These can be overridden via menuconfig or at compile time
 */
#ifndef BUZZ_GPIO
#define BUZZ_GPIO       2   // Input: Optocoupler output (active LOW)
#endif

#ifndef UNLOCK_GPIO
#define UNLOCK_GPIO     3   // Output: PhotoMOS relay control
#endif

/**
 * @brief Timing constants (in milliseconds)
 */
#define DEBOUNCE_TIME_MS        30      // Buzz signal debounce
#define BUZZ_COOLDOWN_MS        2500    // Minimum time between buzz events
#define UNLOCK_PULSE_MS         700     // Duration of unlock pulse
#define UNLOCK_LOCKOUT_MS       1500    // Minimum time between unlock attempts

/**
 * @brief Buzz event callback type
 * Called when a valid (debounced) buzz/ring is detected
 * 
 * @param timestamp_ms System time when buzz was detected
 */
typedef void (*buzz_callback_t)(int64_t timestamp_ms);

/**
 * @brief Initialize GPIO handler
 * 
 * Sets up:
 * - BUZZ_GPIO as input with pull-up, falling edge interrupt
 * - UNLOCK_GPIO as output, initially LOW
 * - Internal timers and synchronization primitives
 * 
 * @param callback Function to call when buzz is detected (can be NULL)
 * @return ESP_OK on success, error code otherwise
 */
esp_err_t gpio_handler_init(buzz_callback_t callback);

/**
 * @brief Trigger door unlock pulse
 * 
 * Pulses UNLOCK_GPIO HIGH for UNLOCK_PULSE_MS duration.
 * Includes lockout to prevent rapid re-triggering.
 * 
 * @return ESP_OK if unlock triggered
 *         ESP_ERR_INVALID_STATE if lockout active
 *         ESP_ERR_TIMEOUT if mutex acquisition failed
 */
esp_err_t gpio_unlock_door(void);

/**
 * @brief Check if unlock is currently in progress or locked out
 * 
 * @return true if unlock cannot be triggered right now
 */
bool gpio_is_unlock_busy(void);

/**
 * @brief Get timestamp of last buzz event
 * 
 * @return Timestamp in milliseconds since boot, or 0 if no buzz detected yet
 */
int64_t gpio_get_last_buzz_time(void);

/**
 * @brief Get timestamp of last unlock event
 * 
 * @return Timestamp in milliseconds since boot, or 0 if no unlock yet
 */
int64_t gpio_get_last_unlock_time(void);

/**
 * @brief Get total buzz count since boot
 * 
 * @return Number of buzz events detected
 */
uint32_t gpio_get_buzz_count(void);

/**
 * @brief Get total unlock count since boot
 * 
 * @return Number of unlock operations performed
 */
uint32_t gpio_get_unlock_count(void);

/**
 * @brief Deinitialize GPIO handler
 * 
 * Releases resources and disables interrupts
 */
void gpio_handler_deinit(void);

#ifdef __cplusplus
}
#endif

#endif // GPIO_HANDLER_H



