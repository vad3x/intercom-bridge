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
 * @brief Emitted by RingActor when hardware ring is detected
 */
struct RingPressedEvent {
    EventMeta meta;
};

/**
 * @brief Emitted by AccessControlActor when unlock is requested
 */
struct UnlockPressedEvent {
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
    Listener<UnlockPressedEvent> unlockPressed;

    // Emit helpers with automatic metadata
    void emitRingPressed(const char* source) {
        ringPressed.emit({EventMeta::now(source)});
    }

    void emitUnlockPressed(const char* source) {
        unlockPressed.emit({EventMeta::now(source)});
    }
};
