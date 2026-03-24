cmake_minimum_required(VERSION 3.13)
set(PICO_NO_PICOTOOL 1)
include(pico_sdk_import.cmake)
project(TuiuiuProject C CXX ASM)
pico_sdk_init()
add_executable(firmware_tui temp_output.cpp)
# Mantenha o extra_outputs logo apos o executable
     pico_add_extra_outputs(firmware_tui)
target_link_libraries(firmware_tui pico_stdlib hardware_flash hardware_watchdog)