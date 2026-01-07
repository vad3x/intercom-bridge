# Smart Intercom ESP-IDF Project
# For ESP32-C6 (DFRobot Beetle ESP32-C6)

cmake_minimum_required(VERSION 3.16)

# Set default IDF_TARGET if not specified
if(NOT DEFINED ENV{IDF_TARGET})
    set(ENV{IDF_TARGET} "esp32c6")
endif()

include($ENV{IDF_PATH}/tools/cmake/project.cmake)

project(intercom-matter)



