/*
   Smart Intercom - Lock Actor

   Listens to Unlocked events and triggers the hardware lock output.
*/

#pragma once

#include "events.h"
#include "hardware_sockets.h"

/**
 * @brief Lock control actor
 *
 * Connects Unlocked events to the LockOutputSocket hardware.
 */
class LockActor {
public:
    /**
     * @brief Initialize lock actor with output socket
     * @param socket Pointer to LockOutputSocket implementation
     *
     * Subscribes to: unlocked
     */
    void init(LockOutputSocket* socket);

private:
    static LockOutputSocket* socket_;

    // Event handler (called by EventBus)
    static void onUnlocked(const UnlockedEvent& e);
};

