idf_component_register(SRC_DIRS "." "./drivers" "./services"
                       PRIV_INCLUDE_DIRS
                         "." "./config" "./events" "./drivers" "./services"
                         "${ESP_MATTER_PATH}/examples/common/utils"
                       REQUIRES esp_matter
                       PRIV_REQUIRES led_strip esp_timer nvs_flash driver)

target_compile_options(${COMPONENT_LIB} PRIVATE "-DCHIP_HAVE_CONFIG_H" "-std=gnu++23")
target_compile_definitions(${COMPONENT_LIB} PRIVATE
  CHIP_DEVICE_CONFIG_DEVICE_VENDOR_NAME="Smart Intercom"
  CHIP_DEVICE_CONFIG_DEVICE_PRODUCT_NAME="Smart Intercom"
)

# Force linker to pull in our emberAfDoorLockClusterInitCallback implementation
# (overrides weak symbol in esp_matter library)
# C++ mangled name for: void emberAfDoorLockClusterInitCallback(uint16_t)
target_link_libraries(${COMPONENT_LIB} INTERFACE "-u _Z34emberAfDoorLockClusterInitCallbackt")
