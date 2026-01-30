/*
   Smart Intercom - Stub LED Driver Implementation
*/

#include "stub_led_driver.h"
#include <esp_log.h>

static const char* TAG = "StubLedDriver";

void StubLedDriver::init() {
    ESP_LOGI(TAG, "LED driver disabled (stub)");
}

void StubLedDriver::blinkRed(uint8_t count, uint32_t onMs, uint32_t offMs) {
    (void)count; (void)onMs; (void)offMs;
}

void StubLedDriver::setColor(uint8_t r, uint8_t g, uint8_t b) {
    (void)r; (void)g; (void)b;
}
