/*
   Smart Intercom - ESP32-C6 Matter Controller

   Configuration definitions from Kconfig.
*/

#pragma once

#include <sdkconfig.h>

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
 * LED Configuration (from Kconfig / sdkconfig)
 */
#ifdef CONFIG_INTERCOM_LED_ENABLED
#define LED_ENABLED CONFIG_INTERCOM_LED_ENABLED
#define LED_GPIO CONFIG_INTERCOM_LED_GPIO
#define LED_BRIGHTNESS CONFIG_INTERCOM_LED_BRIGHTNESS
#else
#define LED_ENABLED 0
#endif
