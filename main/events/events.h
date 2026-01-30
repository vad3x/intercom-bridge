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
 * @brief System lifecycle states
 */
enum class SystemState {
    Initializing,  // Yellow - starting up
    Ready,         // Green - operational
    Error          // Red - something went wrong
};

/**
 * @brief Emitted when system state changes
 */
struct SystemStateEvent {
    EventMeta meta;
    SystemState state;
};

/**
 * @brief Emitted by RingActor when hardware ring is detected
 */
struct RingingEvent {
    EventMeta meta;
};

/**
 * @brief Emitted by AccessControlActor when unlock is dispatched
 */
struct UnlockDispatchedEvent {
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
    Listener<SystemStateEvent> systemState;
    Listener<RingingEvent> ringing;
    Listener<UnlockDispatchedEvent> unlockDispatched;

    // Emit helpers with automatic metadata
    void emitSystemState(const char* source, SystemState state) {
        systemState.emit({EventMeta::now(source), state});
    }

    void emitRinging(const char* source) {
        ringing.emit({EventMeta::now(source)});
    }

    void emitUnlockDispatched(const char* source) {
        unlockDispatched.emit({EventMeta::now(source)});
    }
};
