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
    explicit LockActor(EventBus& bus);

    /**
     * @brief Initialize lock actor with output socket
     * @param socket Pointer to LockOutputSocket implementation
     */
    void init(LockOutputSocket* socket);

private:
    static EventBus* bus_;
    static LockOutputSocket* socket_;

    // Event handler (called by EventBus)
    static void onUnlocked(const UnlockedEvent& e);
};

