/*
   Smart Intercom - Event Definitions

   Event structs with metadata and EventBus using Listener template.
*/

#pragma once

#include "listener.h"
#include <cstdint>
#include <esp_timer.h>

/**
 * @brief Event metadata - included in every event
 */
struct EventMeta {
    int64_t timestamp;   // microseconds since boot
    const char* source;  // actor name that emitted

    static EventMeta now(const char* src) {
        return {esp_timer_get_time(), src};
    }
};

// ============================================================================
// Event Types
// ============================================================================

/**
 * @brief Emitted by RingActor when hardware buzz is detected
 */
struct RingPressedEvent {
    EventMeta meta;
};

/**
 * @brief Emitted by IntercomActor when buzz notification starts
 */
struct BuzzStartedEvent {
    EventMeta meta;
};

/**
 * @brief Emitted by IntercomActor when buzz notification ends (after timeout)
 */
struct BuzzEndedEvent {
    EventMeta meta;
};

/**
 * @brief Emitted by MatterActor when user requests unlock
 */
struct UnlockPressedEvent {
    EventMeta meta;
};

/**
 * @brief Emitted by IntercomActor to trigger the actual unlock
 */
struct UnlockedEvent {
    EventMeta meta;
};

// ============================================================================
// Event Bus
// ============================================================================

/**
 * @brief Central event bus with Listener for each event type
 *
 * Actors subscribe to events in their init() and emit events via helpers.
 */
struct EventBus {
    Listener<RingPressedEvent> ringPressed;
    Listener<BuzzStartedEvent> buzzStarted;
    Listener<BuzzEndedEvent> buzzEnded;
    Listener<UnlockPressedEvent> unlockPressed;
    Listener<UnlockedEvent> unlocked;

    // Emit helpers with automatic metadata
    void emitRingPressed(const char* source) {
        ringPressed.emit({EventMeta::now(source)});
    }

    void emitBuzzStarted(const char* source) {
        buzzStarted.emit({EventMeta::now(source)});
    }

    void emitBuzzEnded(const char* source) {
        buzzEnded.emit({EventMeta::now(source)});
    }

    void emitUnlockPressed(const char* source) {
        unlockPressed.emit({EventMeta::now(source)});
    }

    void emitUnlocked(const char* source) {
        unlocked.emit({EventMeta::now(source)});
    }
};

// Note: EventBus instance is created in app_main.cpp and passed to actors via init()

