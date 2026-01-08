/**
 * @file http_handler.c
 * @brief HTTP server for Smart Intercom remote control
 */

#include "http_handler.h"
#include "gpio_handler.h"
#include "esp_log.h"
#include "esp_timer.h"
#include "cJSON.h"
#include <string.h>

static const char *TAG = "http_handler";

static httpd_handle_t s_server = NULL;

/**
 * @brief Get system uptime in seconds
 */
static inline uint32_t get_uptime_seconds(void)
{
    return (uint32_t)(esp_timer_get_time() / 1000000);
}

/**
 * @brief Handler for POST /unlock
 */
static esp_err_t unlock_handler(httpd_req_t *req)
{
    ESP_LOGI(TAG, "POST /unlock request received");
    
    cJSON *response = cJSON_CreateObject();
    if (response == NULL) {
        httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR, "JSON allocation failed");
        return ESP_FAIL;
    }
    
    esp_err_t ret = gpio_unlock_door();
    
    if (ret == ESP_OK) {
        cJSON_AddBoolToObject(response, "success", true);
        cJSON_AddStringToObject(response, "message", "Door unlock triggered");
        cJSON_AddNumberToObject(response, "pulse_ms", UNLOCK_PULSE_MS);
    } else if (ret == ESP_ERR_INVALID_STATE) {
        cJSON_AddBoolToObject(response, "success", false);
        if (gpio_is_unlock_busy()) {
            cJSON_AddStringToObject(response, "error", "Unlock in progress or lockout active");
            int64_t last_unlock = gpio_get_last_unlock_time();
            int64_t now = esp_timer_get_time() / 1000;
            int64_t remaining = UNLOCK_LOCKOUT_MS - (now - last_unlock);
            if (remaining > 0) {
                cJSON_AddNumberToObject(response, "retry_after_ms", remaining);
            }
        } else {
            cJSON_AddStringToObject(response, "error", "Unlock not available");
        }
    } else {
        cJSON_AddBoolToObject(response, "success", false);
        cJSON_AddStringToObject(response, "error", esp_err_to_name(ret));
    }
    
    const char *json_str = cJSON_PrintUnformatted(response);
    if (json_str == NULL) {
        cJSON_Delete(response);
        httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR, "JSON serialization failed");
        return ESP_FAIL;
    }
    
    httpd_resp_set_type(req, "application/json");
    httpd_resp_set_hdr(req, "Access-Control-Allow-Origin", "*");
    httpd_resp_send(req, json_str, strlen(json_str));
    
    cJSON_free((void *)json_str);
    cJSON_Delete(response);
    
    return ESP_OK;
}

/**
 * @brief Handler for GET /status
 */
static esp_err_t status_handler(httpd_req_t *req)
{
    ESP_LOGD(TAG, "GET /status request received");
    
    cJSON *response = cJSON_CreateObject();
    if (response == NULL) {
        httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR, "JSON allocation failed");
        return ESP_FAIL;
    }
    
    // System info
    cJSON_AddStringToObject(response, "device", "smart-intercom");
    cJSON_AddStringToObject(response, "version", "1.0.0");
    cJSON_AddNumberToObject(response, "uptime_seconds", get_uptime_seconds());
    
    // Buzz stats
    cJSON *buzz = cJSON_CreateObject();
    cJSON_AddNumberToObject(buzz, "count", gpio_get_buzz_count());
    cJSON_AddNumberToObject(buzz, "last_time_ms", gpio_get_last_buzz_time());
    cJSON_AddItemToObject(response, "buzz", buzz);
    
    // Unlock stats
    cJSON *unlock = cJSON_CreateObject();
    cJSON_AddNumberToObject(unlock, "count", gpio_get_unlock_count());
    cJSON_AddNumberToObject(unlock, "last_time_ms", gpio_get_last_unlock_time());
    cJSON_AddBoolToObject(unlock, "busy", gpio_is_unlock_busy());
    cJSON_AddItemToObject(response, "unlock", unlock);
    
    // Configuration
    cJSON *config = cJSON_CreateObject();
    cJSON_AddNumberToObject(config, "buzz_gpio", BUZZ_GPIO);
    cJSON_AddNumberToObject(config, "unlock_gpio", UNLOCK_GPIO);
    cJSON_AddNumberToObject(config, "unlock_pulse_ms", UNLOCK_PULSE_MS);
    cJSON_AddNumberToObject(config, "unlock_lockout_ms", UNLOCK_LOCKOUT_MS);
    cJSON_AddNumberToObject(config, "buzz_cooldown_ms", BUZZ_COOLDOWN_MS);
    cJSON_AddNumberToObject(config, "debounce_ms", DEBOUNCE_TIME_MS);
    cJSON_AddItemToObject(response, "config", config);
    
    const char *json_str = cJSON_PrintUnformatted(response);
    if (json_str == NULL) {
        cJSON_Delete(response);
        httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR, "JSON serialization failed");
        return ESP_FAIL;
    }
    
    httpd_resp_set_type(req, "application/json");
    httpd_resp_set_hdr(req, "Access-Control-Allow-Origin", "*");
    httpd_resp_send(req, json_str, strlen(json_str));
    
    cJSON_free((void *)json_str);
    cJSON_Delete(response);
    
    return ESP_OK;
}

/**
 * @brief Handler for OPTIONS requests (CORS preflight)
 */
static esp_err_t options_handler(httpd_req_t *req)
{
    httpd_resp_set_hdr(req, "Access-Control-Allow-Origin", "*");
    httpd_resp_set_hdr(req, "Access-Control-Allow-Methods", "GET, POST, OPTIONS");
    httpd_resp_set_hdr(req, "Access-Control-Allow-Headers", "Content-Type");
    httpd_resp_set_hdr(req, "Access-Control-Max-Age", "86400");
    httpd_resp_send(req, NULL, 0);
    return ESP_OK;
}

// URI handler definitions
static const httpd_uri_t uri_unlock = {
    .uri       = "/unlock",
    .method    = HTTP_POST,
    .handler   = unlock_handler,
    .user_ctx  = NULL
};

static const httpd_uri_t uri_status = {
    .uri       = "/status",
    .method    = HTTP_GET,
    .handler   = status_handler,
    .user_ctx  = NULL
};

static const httpd_uri_t uri_options_unlock = {
    .uri       = "/unlock",
    .method    = HTTP_OPTIONS,
    .handler   = options_handler,
    .user_ctx  = NULL
};

static const httpd_uri_t uri_options_status = {
    .uri       = "/status",
    .method    = HTTP_OPTIONS,
    .handler   = options_handler,
    .user_ctx  = NULL
};

esp_err_t http_server_start(void)
{
    if (s_server != NULL) {
        ESP_LOGW(TAG, "HTTP server already running");
        return ESP_ERR_INVALID_STATE;
    }
    
    httpd_config_t config = HTTPD_DEFAULT_CONFIG();
    config.server_port = HTTP_SERVER_PORT;
    config.max_uri_handlers = 8;
    config.stack_size = 8192;
    config.lru_purge_enable = true;
    
    ESP_LOGI(TAG, "Starting HTTP server on port %d", config.server_port);
    
    esp_err_t ret = httpd_start(&s_server, &config);
    if (ret != ESP_OK) {
        ESP_LOGE(TAG, "Failed to start HTTP server: %s", esp_err_to_name(ret));
        return ret;
    }
    
    // Register URI handlers
    ESP_LOGI(TAG, "Registering URI handlers");
    
    ret = httpd_register_uri_handler(s_server, &uri_unlock);
    if (ret != ESP_OK) {
        ESP_LOGE(TAG, "Failed to register /unlock handler");
        httpd_stop(s_server);
        s_server = NULL;
        return ret;
    }
    
    ret = httpd_register_uri_handler(s_server, &uri_status);
    if (ret != ESP_OK) {
        ESP_LOGE(TAG, "Failed to register /status handler");
        httpd_stop(s_server);
        s_server = NULL;
        return ret;
    }
    
    // Register CORS OPTIONS handlers
    httpd_register_uri_handler(s_server, &uri_options_unlock);
    httpd_register_uri_handler(s_server, &uri_options_status);
    
    ESP_LOGI(TAG, "HTTP server started successfully");
    ESP_LOGI(TAG, "  POST /unlock - Trigger door unlock");
    ESP_LOGI(TAG, "  GET  /status - Get system status");
    
    return ESP_OK;
}

esp_err_t http_server_stop(void)
{
    if (s_server == NULL) {
        ESP_LOGW(TAG, "HTTP server not running");
        return ESP_ERR_INVALID_STATE;
    }
    
    ESP_LOGI(TAG, "Stopping HTTP server");
    
    esp_err_t ret = httpd_stop(s_server);
    s_server = NULL;
    
    if (ret != ESP_OK) {
        ESP_LOGE(TAG, "Failed to stop HTTP server: %s", esp_err_to_name(ret));
    } else {
        ESP_LOGI(TAG, "HTTP server stopped");
    }
    
    return ret;
}

bool http_server_is_running(void)
{
    return s_server != NULL;
}

httpd_handle_t http_server_get_handle(void)
{
    return s_server;
}
