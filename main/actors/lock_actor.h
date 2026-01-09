/*
   Smart Intercom - Lock Actor

   Listens to Unlocked events and triggers the hardware lock output.
*/

#pragma once

#include "events/events.h"
#include "hardware/hardware_sockets.h"

/**
 * @brief Lock control actor
 *
 * Connects Unlocked events to the LockOutputSocket hardware.
 */
class LockActor {
public:
    /**
     * @brief Initialize lock actor with output socket
     * @param bus Reference to EventBus for subscribing
     * @param socket Pointer to LockOutputSocket implementation
     *
     * Subscribes to: unlocked
     */
    void init(EventBus& bus, LockOutputSocket* socket);

private:
    static LockOutputSocket* socket_;

    // Event handler (called by EventBus)
    static void onUnlocked(const UnlockedEvent& e);
};

