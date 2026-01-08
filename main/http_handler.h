/**
 * @file http_handler.h
 * @brief HTTP server for Smart Intercom remote control
 * 
 * Endpoints:
 * - POST /unlock  - Trigger door unlock
 * - GET  /status  - Get system status (buzz count, timestamps, etc.)
 */

#ifndef HTTP_HANDLER_H
#define HTTP_HANDLER_H

#include "esp_err.h"
#include "esp_http_server.h"

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @brief HTTP server configuration
 */
#ifndef HTTP_SERVER_PORT
#define HTTP_SERVER_PORT    80
#endif

/**
 * @brief Start HTTP server
 * 
 * Creates and starts the HTTP server with registered URI handlers.
 * The server runs on HTTP_SERVER_PORT (default 80).
 * 
 * @return ESP_OK on success, error code otherwise
 */
esp_err_t http_server_start(void);

/**
 * @brief Stop HTTP server
 * 
 * Gracefully stops the HTTP server and releases resources.
 * 
 * @return ESP_OK on success, error code otherwise
 */
esp_err_t http_server_stop(void);

/**
 * @brief Check if HTTP server is running
 * 
 * @return true if server is running
 */
bool http_server_is_running(void);

/**
 * @brief Get HTTP server handle
 * 
 * Useful for adding custom handlers or configuration.
 * 
 * @return Server handle, or NULL if not running
 */
httpd_handle_t http_server_get_handle(void);

#ifdef __cplusplus
}
#endif

#endif // HTTP_HANDLER_H
