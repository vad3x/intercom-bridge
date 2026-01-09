idf_component_register(SRC_DIRS "." "./intercom"
                       PRIV_INCLUDE_DIRS
                         "." "./intercom" "${ESP_MATTER_PATH}/examples/common/utils")

target_compile_options(${COMPONENT_LIB} PRIVATE "-DCHIP_HAVE_CONFIG_H")

# Force linker to pull in our emberAfDoorLockClusterInitCallback implementation
# (overrides weak symbol in esp_matter library)
# C++ mangled name for: void emberAfDoorLockClusterInitCallback(uint16_t)
target_link_libraries(${COMPONENT_LIB} INTERFACE "-u _Z34emberAfDoorLockClusterInitCallbackt")
