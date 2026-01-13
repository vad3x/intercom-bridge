# Distributed under the OSI-approved BSD 3-Clause License.  See accompanying
# file Copyright.txt or https://cmake.org/licensing for details.

cmake_minimum_required(VERSION 3.5)

file(MAKE_DIRECTORY
  "/opt/espressif/esp-idf/components/bootloader/subproject"
  "/opt/espressif/esp-idf/examples/get-started/hello_world/build/bootloader"
  "/opt/espressif/esp-idf/examples/get-started/hello_world/build/bootloader-prefix"
  "/opt/espressif/esp-idf/examples/get-started/hello_world/build/bootloader-prefix/tmp"
  "/opt/espressif/esp-idf/examples/get-started/hello_world/build/bootloader-prefix/src/bootloader-stamp"
  "/opt/espressif/esp-idf/examples/get-started/hello_world/build/bootloader-prefix/src"
  "/opt/espressif/esp-idf/examples/get-started/hello_world/build/bootloader-prefix/src/bootloader-stamp"
)

set(configSubDirs )
foreach(subDir IN LISTS configSubDirs)
    file(MAKE_DIRECTORY "/opt/espressif/esp-idf/examples/get-started/hello_world/build/bootloader-prefix/src/bootloader-stamp/${subDir}")
endforeach()
if(cfgdir)
  file(MAKE_DIRECTORY "/opt/espressif/esp-idf/examples/get-started/hello_world/build/bootloader-prefix/src/bootloader-stamp${cfgdir}") # cfgdir has leading slash
endif()
