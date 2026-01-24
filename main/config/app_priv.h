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
 * GPIO Configuration (from Kconfig / sdkconfig)
 */
#define RING_GPIO CONFIG_INTERCOM_RING_GPIO
#define UNLOCK_GPIO CONFIG_INTERCOM_UNLOCK_GPIO

/*
 * Timing Constants (from Kconfig / sdkconfig)
 */
#define UNLOCK_PULSE_MS CONFIG_INTERCOM_UNLOCK_PULSE_MS
#define RING_DEBOUNCE_MS CONFIG_INTERCOM_RING_DEBOUNCE_MS
#define RING_COOLDOWN_MS CONFIG_INTERCOM_RING_COOLDOWN_MS
#define RING_HOLD_MS CONFIG_INTERCOM_RING_HOLD_MS
#define AUTO_RELOCK_SEC CONFIG_INTERCOM_AUTO_RELOCK_SEC

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
