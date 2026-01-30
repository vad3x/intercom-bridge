/*
   Smart Intercom - Stub LED Driver

   No-op implementation satisfying LedDriver concept for boards without LEDs.
*/

#pragma once

#include <cstdint>

class StubLedDriver {
public:
    StubLedDriver() = default;

    void init();
    void yellow() {}
    void green() {}
    void red() {}
    void off() {}
    void blinkRed(uint8_t count, uint32_t onMs = 100, uint32_t offMs = 100);
    void setColor(uint8_t r, uint8_t g, uint8_t b);
};
