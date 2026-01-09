/*
   Smart Intercom - ESP32-C6 Matter Controller

   Configuration and shared definitions.
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
#define BUZZ_GPIO 2    // Input: Optocoupler output (active LOW when ringing)
#define UNLOCK_GPIO 3  // Output: PhotoMOS relay control (active HIGH)

/*
 * Timing Constants (milliseconds)
 */
#define UNLOCK_PULSE_MS 700    // Duration of relay activation
#define DEBOUNCE_MS 30         // Buzz signal debounce time
#define BUZZ_COOLDOWN_MS 2500  // Minimum time between buzz events
#define BUZZ_HOLD_MS 4000      // How long button stays "pressed" in HomeKit

/*
 * Legacy driver interface (for compatibility)
 */
typedef void* app_driver_handle_t;

#if CHIP_DEVICE_CONFIG_ENABLE_THREAD
#define ESP_OPENTHREAD_DEFAULT_RADIO_CONFIG() \
    { .radio_mode = RADIO_MODE_NATIVE, }

#define ESP_OPENTHREAD_DEFAULT_HOST_CONFIG() \
    { .host_connection_mode = HOST_CONNECTION_MODE_NONE, }

#define ESP_OPENTHREAD_DEFAULT_PORT_CONFIG() \
    { .storage_partition_name = "nvs", .netif_queue_size = 10, .task_queue_size = 10, }
#endif
