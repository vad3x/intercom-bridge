/*
   Smart Intercom - ESP32-C6 Matter Controller
   
   Simplified door lock with contact sensor for buzz detection.
   No PIN codes, users, or schedules - just lock/unlock and ring notification.
*/

#pragma once

#include <esp_err.h>
#include <esp_matter.h>
#include <stdint.h>

#if CHIP_DEVICE_CONFIG_ENABLE_THREAD
#include "esp_openthread_types.h"
#endif

/*
 * GPIO Configuration
 */
#define BUZZ_GPIO           2   // Input: Optocoupler output (active LOW when ringing)
#define UNLOCK_GPIO         3   // Output: PhotoMOS relay control (active HIGH)

/*
 * Timing Constants (milliseconds)
 */
#define UNLOCK_PULSE_MS     700     // Duration of relay activation
#define DEBOUNCE_MS         30      // Buzz signal debounce time
#define BUZZ_COOLDOWN_MS    2500    // Minimum time between buzz events
#define BUZZ_ACTIVE_MS      2000    // How long contact sensor stays "open"

/*
 * Endpoint IDs (set in app_main.cpp)
 */
extern uint16_t door_lock_endpoint_id;
extern uint16_t contact_sensor_endpoint_id;

typedef void *app_driver_handle_t;

/** Initialize the button driver
 *
 * This initializes the button driver associated with the selected board.
 *
 * @return Handle on success.
 * @return NULL in case of failure.
 */
app_driver_handle_t app_driver_button_init();

/** Driver Update
 *
 * This API should be called to update the driver for the attribute being updated.
 * This is usually called from the common `app_attribute_update_cb()`.
 *
 * @param[in] endpoint_id Endpoint ID of the attribute.
 * @param[in] cluster_id Cluster ID of the attribute.
 * @param[in] attribute_id Attribute ID of the attribute.
 * @param[in] val Pointer to `esp_matter_attr_val_t`. Use appropriate elements as per the value type.
 *
 * @return ESP_OK on success.
 * @return error in case of failure.
 */
esp_err_t app_driver_attribute_update(app_driver_handle_t driver_handle, uint16_t endpoint_id, uint32_t cluster_id,
                                      uint32_t attribute_id, esp_matter_attr_val_t *val);

/** Initialize door lock callbacks
 * Resolves linking issue with callbacks in door_lock_callbacks.cpp
 */
void door_lock_init();

/** Initialize GPIO for intercom hardware
 * Sets up UNLOCK_GPIO as output and BUZZ_GPIO as input with interrupt
 * 
 * @return ESP_OK on success
 */
esp_err_t app_driver_gpio_init();

/** Trigger door unlock pulse
 * Pulses UNLOCK_GPIO HIGH for UNLOCK_PULSE_MS duration
 * 
 * @return ESP_OK on success, ESP_ERR_INVALID_STATE if pulse already in progress
 */
esp_err_t app_driver_unlock_pulse();

/** Check if unlock pulse is in progress
 * @return true if relay is currently activated
 */
bool app_driver_is_unlocking();

#if CHIP_DEVICE_CONFIG_ENABLE_THREAD
#define ESP_OPENTHREAD_DEFAULT_RADIO_CONFIG()                                           \
    {                                                                                   \
        .radio_mode = RADIO_MODE_NATIVE,                                                \
    }

#define ESP_OPENTHREAD_DEFAULT_HOST_CONFIG()                                            \
    {                                                                                   \
        .host_connection_mode = HOST_CONNECTION_MODE_NONE,                              \
    }

#define ESP_OPENTHREAD_DEFAULT_PORT_CONFIG()                                            \
    {                                                                                   \
        .storage_partition_name = "nvs", .netif_queue_size = 10, .task_queue_size = 10, \
    }
#endif
