/*
   Smart Intercom - Indicator Actor

   Template actor that observes business events and orchestrates visual/audio
   feedback. Implements the observer pattern - other actors have no knowledge
   of indicator behavior.
*/

#pragma once

#include "events/events.h"
#include <concepts>
#include <esp_log.h>

// ============================================================================
// IndicatorDriver Concept
// ============================================================================

/**
 * @brief Concept for indicator drivers (LED, buzzer, etc.)
 *
 * Driver must implement:
 *   void init()
 *   void yellow()   - Initializing state
 *   void green()    - Ready state
 *   void red()      - Error state
 *   void off()      - Indicator off
 *   void blinkRed(uint8_t count, uint32_t onMs, uint32_t offMs)
 *   void setColor(uint8_t r, uint8_t g, uint8_t b)
 */
template <typename Driver>
concept IndicatorDriver = requires(Driver driver, uint8_t count, uint32_t durationMs, uint8_t colorValue) {
    driver.init();
    driver.yellow();
    driver.green();
    driver.red();
    driver.off();
    driver.blinkRed(count, durationMs, durationMs);
    driver.setColor(colorValue, colorValue, colorValue);
};

// ============================================================================
// IndicatorActor Template
// ============================================================================

/**
 * @brief Indicator actor - observer pattern
 *
 * Observes events and orchestrates feedback:
 *   - SystemStateEvent: Yellow/Green/Red based on state
 *   - RingingEvent: Blink red 2x
 *   - UnlockDispatchedEvent: Blink red 2x
 *
 * @tparam Driver Type satisfying IndicatorDriver concept
 */
template <IndicatorDriver Driver>
class IndicatorActor {
    static constexpr const char* TAG = "IndicatorActor";

   public:
    IndicatorActor(EventBus& bus, Driver& driver) : bus_(bus), driver_(driver) {}

    void init() {
        ESP_LOGI(TAG, "Initializing IndicatorActor");
        driver_.init();

        bus_.systemState.subscribe([this](const SystemStateEvent& e) { onSystemState(e); });
        bus_.ringing.subscribe([this](const RingingEvent& e) { onRinging(e); });
        bus_.unlockDispatched.subscribe([this](const UnlockDispatchedEvent& e) { onUnlockDispatched(e); });

        ESP_LOGI(TAG, "Subscribed to events");
    }

   private:
    EventBus& bus_;
    Driver& driver_;

    void onSystemState(const SystemStateEvent& e) {
        switch (e.state) {
            case SystemState::Initializing:
                ESP_LOGI(TAG, "System initializing - yellow");
                driver_.yellow();
                break;
            case SystemState::Ready:
                ESP_LOGI(TAG, "System ready - green");
                driver_.green();
                break;
            case SystemState::Error:
                ESP_LOGI(TAG, "System error - red");
                driver_.red();
                break;
        }
    }

    void onRinging(const RingingEvent& e) {
        ESP_LOGI(TAG, "Ring detected - blinking");
        driver_.blinkRed(2);
    }

    void onUnlockDispatched(const UnlockDispatchedEvent& e) {
        ESP_LOGI(TAG, "Unlock dispatched - blinking");
        driver_.blinkRed(2);
    }
};
