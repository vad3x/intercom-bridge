/*
 * Smart Intercom - CHIP/Matter Project Configuration
 *
 * This file overrides default CHIP device configuration values.
 * See: https://github.com/project-chip/connectedhomeip/blob/master/src/include/platform/CHIPDeviceConfig.h
 */

#pragma once

/**
 * CHIP_DEVICE_CONFIG_DEVICE_VENDOR_NAME
 *
 * Human-readable vendor name for the device.
 */
#ifdef CHIP_DEVICE_CONFIG_DEVICE_VENDOR_NAME
#undef CHIP_DEVICE_CONFIG_DEVICE_VENDOR_NAME
#endif
#define CHIP_DEVICE_CONFIG_DEVICE_VENDOR_NAME "Smart Intercom"

/**
 * CHIP_DEVICE_CONFIG_DEVICE_PRODUCT_NAME
 *
 * Human-readable product name for the device.
 */
#ifdef CHIP_DEVICE_CONFIG_DEVICE_PRODUCT_NAME
#undef CHIP_DEVICE_CONFIG_DEVICE_PRODUCT_NAME
#endif
#define CHIP_DEVICE_CONFIG_DEVICE_PRODUCT_NAME "Smart Intercom"

/**
 * CHIP_DEVICE_CONFIG_DEVICE_VENDOR_ID
 *
 * 0xFFF1 is the test vendor ID assigned by CSA for development.
 */
#ifdef CHIP_DEVICE_CONFIG_DEVICE_VENDOR_ID
#undef CHIP_DEVICE_CONFIG_DEVICE_VENDOR_ID
#endif
#define CHIP_DEVICE_CONFIG_DEVICE_VENDOR_ID 0xFFF1

/**
 * CHIP_DEVICE_CONFIG_DEVICE_PRODUCT_ID
 *
 * Product ID for this device (test range).
 */
#ifdef CHIP_DEVICE_CONFIG_DEVICE_PRODUCT_ID
#undef CHIP_DEVICE_CONFIG_DEVICE_PRODUCT_ID
#endif
#define CHIP_DEVICE_CONFIG_DEVICE_PRODUCT_ID 0x8001

/**
 * CHIP_DEVICE_CONFIG_DEFAULT_DEVICE_HARDWARE_VERSION_STRING
 */
#ifdef CHIP_DEVICE_CONFIG_DEFAULT_DEVICE_HARDWARE_VERSION_STRING
#undef CHIP_DEVICE_CONFIG_DEFAULT_DEVICE_HARDWARE_VERSION_STRING
#endif
#define CHIP_DEVICE_CONFIG_DEFAULT_DEVICE_HARDWARE_VERSION_STRING "v1.0"

/**
 * CHIP_DEVICE_CONFIG_DEVICE_SOFTWARE_VERSION_STRING
 */
#ifdef CHIP_DEVICE_CONFIG_DEVICE_SOFTWARE_VERSION_STRING
#undef CHIP_DEVICE_CONFIG_DEVICE_SOFTWARE_VERSION_STRING
#endif
#define CHIP_DEVICE_CONFIG_DEVICE_SOFTWARE_VERSION_STRING "1.0.0"

/**
 * CHIP_DEVICE_CONFIG_DEVICE_SOFTWARE_VERSION
 */
#ifdef CHIP_DEVICE_CONFIG_DEVICE_SOFTWARE_VERSION
#undef CHIP_DEVICE_CONFIG_DEVICE_SOFTWARE_VERSION
#endif
#define CHIP_DEVICE_CONFIG_DEVICE_SOFTWARE_VERSION 1

/**
 * OpenThread Configuration (only when Thread is enabled)
 */
#if CHIP_DEVICE_CONFIG_ENABLE_THREAD
#include "esp_openthread_types.h"

#define ESP_OPENTHREAD_DEFAULT_RADIO_CONFIG() \
    { .radio_mode = RADIO_MODE_NATIVE, }

#define ESP_OPENTHREAD_DEFAULT_HOST_CONFIG() \
    { .host_connection_mode = HOST_CONNECTION_MODE_NONE, }

#define ESP_OPENTHREAD_DEFAULT_PORT_CONFIG() \
    { .storage_partition_name = "nvs", .netif_queue_size = 10, .task_queue_size = 10, }
#endif
