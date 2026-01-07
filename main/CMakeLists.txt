# Main component CMakeLists.txt

idf_component_register(
    SRCS
        "main.c"
        "gpio_handler.c"
        "http_handler.c"
    INCLUDE_DIRS
        "."
    REQUIRES
        driver
        esp_wifi
        esp_event
        esp_netif
        nvs_flash
        esp_http_server
        json
)



