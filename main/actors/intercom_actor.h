/*
   Smart Intercom - Intercom Actor

   Core business logic actor.
   - Listens to RingPressed → emits BuzzStarted, then BuzzEnded after timeout
   - Listens to UnlockPressed → emits Unlocked
*/

#pragma once

#include "events/events.h"
#include <freertos/FreeRTOS.h>
#include <freertos/timers.h>

/**
 * @brief Core business logic actor
 *
 * Handles the intercom logic:
 * - Ring detection triggers buzz notification with timeout
 * - Unlock request triggers actual unlock
 */
class IntercomActor {
public:
    IntercomActor(EventBus& bus, uint32_t buzzHoldMs);

    void init();

private:
    static EventBus* bus_;
    static uint32_t buzzHoldMs_;
    static TimerHandle_t buzzTimer_;

    // Event handlers (called by EventBus)
    static void onRingPressed(const RingPressedEvent& e);
    static void onUnlockPressed(const UnlockPressedEvent& e);

    // Timer callback for buzz timeout
    static void buzzTimerCallback(TimerHandle_t timer);
};

