/*
   Smart Intercom - ESP32-C6 Matter Controller

   Configuration and shared definitions.
*/

#pragma once

#include <esp_err.h>
#include <esp_matter.h>
#include <stdint.h>

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

