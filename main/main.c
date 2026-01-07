/**
 * @file main.c
 * @brief Smart Intercom main application
 * 
 * ESP32-C6 firmware for IR204E intercom integration:
 * - WiFi connectivity
 * - HTTP API for remote control
 * - Buzz/ring detection via optocoupler
 * - Door unlock via PhotoMOS relay
 */

#include <stdio.h>
#include <string.h>
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "freertos/event_groups.h"
#include "esp_system.h"
#include "esp_wifi.h"
#include "esp_event.h"
#include "esp_log.h"
#include "esp_netif.h"
#include "nvs_flash.h"

#include "gpio_handler.h"
#include "http_handler.h"

static const char *TAG = "main";

/**
 * @brief WiFi configuration
 * TODO: Move to menuconfig or provisioning
 */
#ifndef WIFI_SSID
#define WIFI_SSID       "vs_us"
#endif

#ifndef WIFI_PASSWORD
#define WIFI_PASSWORD   "h993778L"
#endif

#define WIFI_MAXIMUM_RETRY  10

// Event group for WiFi events
static EventGroupHandle_t s_wifi_event_group;

#define WIFI_CONNECTED_BIT  BIT0
#define WIFI_FAIL_BIT       BIT1

static int s_retry_num = 0;

/**
 * @brief WiFi event handler
 */
static void wifi_event_handler(void *arg, esp_event_base_t event_base,
                               int32_t event_id, void *event_data)
{
    if (event_base == WIFI_EVENT && event_id == WIFI_EVENT_STA_START) {
        esp_wifi_connect();
    } else if (event_base == WIFI_EVENT && event_id == WIFI_EVENT_STA_DISCONNECTED) {
        if (s_retry_num < WIFI_MAXIMUM_RETRY) {
            esp_wifi_connect();
            s_retry_num++;
            ESP_LOGI(TAG, "Retrying WiFi connection (attempt %d/%d)", 
                     s_retry_num, WIFI_MAXIMUM_RETRY);
        } else {
            xEventGroupSetBits(s_wifi_event_group, WIFI_FAIL_BIT);
            ESP_LOGE(TAG, "WiFi connection failed after %d attempts", WIFI_MAXIMUM_RETRY);
        }
    } else if (event_base == IP_EVENT && event_id == IP_EVENT_STA_GOT_IP) {
        ip_event_got_ip_t *event = (ip_event_got_ip_t *)event_data;
        ESP_LOGI(TAG, "Got IP address: " IPSTR, IP2STR(&event->ip_info.ip));
        s_retry_num = 0;
        xEventGroupSetBits(s_wifi_event_group, WIFI_CONNECTED_BIT);
    }
}

/**
 * @brief Initialize WiFi in station mode
 */
static esp_err_t wifi_init_sta(void)
{
    ESP_LOGI(TAG, "Initializing WiFi");
    
    s_wifi_event_group = xEventGroupCreate();
    
    ESP_ERROR_CHECK(esp_netif_init());
    ESP_ERROR_CHECK(esp_event_loop_create_default());
    esp_netif_create_default_wifi_sta();
    
    wifi_init_config_t cfg = WIFI_INIT_CONFIG_DEFAULT();
    ESP_ERROR_CHECK(esp_wifi_init(&cfg));
    
    // Register event handlers
    esp_event_handler_instance_t instance_any_id;
    esp_event_handler_instance_t instance_got_ip;
    ESP_ERROR_CHECK(esp_event_handler_instance_register(
        WIFI_EVENT, ESP_EVENT_ANY_ID, &wifi_event_handler, NULL, &instance_any_id));
    ESP_ERROR_CHECK(esp_event_handler_instance_register(
        IP_EVENT, IP_EVENT_STA_GOT_IP, &wifi_event_handler, NULL, &instance_got_ip));
    
    // Configure WiFi
    wifi_config_t wifi_config = {
        .sta = {
            .ssid = WIFI_SSID,
            .password = WIFI_PASSWORD,
            .threshold.authmode = WIFI_AUTH_WPA2_PSK,
            .sae_pwe_h2e = WPA3_SAE_PWE_BOTH,
        },
    };
    
    ESP_ERROR_CHECK(esp_wifi_set_mode(WIFI_MODE_STA));
    ESP_ERROR_CHECK(esp_wifi_set_config(WIFI_IF_STA, &wifi_config));
    ESP_ERROR_CHECK(esp_wifi_start());
    
    ESP_LOGI(TAG, "WiFi initialization complete, connecting to SSID: %s", WIFI_SSID);
    
    // Wait for connection or failure
    EventBits_t bits = xEventGroupWaitBits(
        s_wifi_event_group,
        WIFI_CONNECTED_BIT | WIFI_FAIL_BIT,
        pdFALSE,
        pdFALSE,
        portMAX_DELAY
    );
    
    if (bits & WIFI_CONNECTED_BIT) {
        ESP_LOGI(TAG, "WiFi connected successfully");
        return ESP_OK;
    } else if (bits & WIFI_FAIL_BIT) {
        ESP_LOGE(TAG, "WiFi connection failed");
        return ESP_FAIL;
    }
    
    ESP_LOGE(TAG, "Unexpected WiFi event");
    return ESP_FAIL;
}

/**
 * @brief Callback when buzz/ring is detected
 */
static void on_buzz_detected(int64_t timestamp_ms)
{
    ESP_LOGI(TAG, "=== BUZZ/RING DETECTED ===");
    ESP_LOGI(TAG, "Timestamp: %lld ms since boot", timestamp_ms);
    
    // Future: Send notification via Matter, MQTT, or webhook
    // For now, just log the event
}

/**
 * @brief Initialize Non-Volatile Storage
 */
static esp_err_t nvs_init(void)
{
    esp_err_t ret = nvs_flash_init();
    if (ret == ESP_ERR_NVS_NO_FREE_PAGES || ret == ESP_ERR_NVS_NEW_VERSION_FOUND) {
        ESP_LOGW(TAG, "NVS partition needs erase");
        ESP_ERROR_CHECK(nvs_flash_erase());
        ret = nvs_flash_init();
    }
    return ret;
}

/**
 * @brief Main application entry point
 */
void app_main(void)
{
    ESP_LOGI(TAG, "=========================================");
    ESP_LOGI(TAG, "   Smart Intercom - Starting Up");
    ESP_LOGI(TAG, "=========================================");
    ESP_LOGI(TAG, "ESP-IDF version: %s", esp_get_idf_version());
    
    // Initialize NVS
    ESP_LOGI(TAG, "Initializing NVS...");
    ESP_ERROR_CHECK(nvs_init());
    
    // Initialize GPIO handler
    ESP_LOGI(TAG, "Initializing GPIO handler...");
    ESP_ERROR_CHECK(gpio_handler_init(on_buzz_detected));
    
    // Initialize WiFi
    ESP_LOGI(TAG, "Connecting to WiFi...");
    esp_err_t wifi_ret = wifi_init_sta();
    
    if (wifi_ret == ESP_OK) {
        // Start HTTP server only if WiFi is connected
        ESP_LOGI(TAG, "Starting HTTP server...");
        ESP_ERROR_CHECK(http_server_start());
        
        ESP_LOGI(TAG, "=========================================");
        ESP_LOGI(TAG, "   Smart Intercom - Ready!");
        ESP_LOGI(TAG, "=========================================");
        ESP_LOGI(TAG, "HTTP API endpoints:");
        ESP_LOGI(TAG, "  POST /unlock - Trigger door unlock");
        ESP_LOGI(TAG, "  GET  /status - Get system status");
        ESP_LOGI(TAG, "=========================================");
    } else {
        ESP_LOGE(TAG, "WiFi connection failed - HTTP server not started");
        ESP_LOGE(TAG, "System running in limited mode (GPIO only)");
    }
    
    // Main monitoring loop
    while (1) {
        // Periodic status output (every 60 seconds)
        vTaskDelay(pdMS_TO_TICKS(60000));
        
        ESP_LOGI(TAG, "Status: Uptime=%lld s, Buzz=%lu, Unlock=%lu",
                 esp_timer_get_time() / 1000000,
                 (unsigned long)gpio_get_buzz_count(),
                 (unsigned long)gpio_get_unlock_count());
    }
}



