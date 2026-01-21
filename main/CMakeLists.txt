idf_component_register(SRC_DIRS "." "./drivers" "./services"
                       PRIV_INCLUDE_DIRS
                         "." "./config" "./events" "./drivers" "./services"
                         "${ESP_MATTER_PATH}/examples/common/utils"
                       REQUIRES esp_matter
                       PRIV_REQUIRES led_strip esp_timer nvs_flash driver)

target_compile_options(${COMPONENT_LIB} PRIVATE "-DCHIP_HAVE_CONFIG_H" "-std=gnu++23")

# Force linker to pull in our emberAfDoorLockClusterInitCallback implementation
# (overrides weak symbol in esp_matter library)
# C++ mangled name for: void emberAfDoorLockClusterInitCallback(uint16_t)
target_link_libraries(${COMPONENT_LIB} INTERFACE "-u _Z34emberAfDoorLockClusterInitCallbackt")
