/*
   Smart Intercom - Ring Actor

   Listens to hardware buzz detection and emits RingPressed events.
*/

#pragma once

#include "hardware_sockets.h"

struct EventBus;  // Forward declaration

/**
 * @brief Ring detection actor
 *
 * Connects BuzzInputSocket to the event bus.
 * When hardware detects a buzz, emits RingPressed event.
 */
class RingActor {
public:
    /**
     * @brief Initialize ring actor with buzz input socket
     * @param bus Reference to EventBus for emitting events
     * @param socket Pointer to BuzzInputSocket implementation
     */
    void init(EventBus& bus, BuzzInputSocket* socket);

private:
    static EventBus* bus_;

    /**
     * @brief Called by BuzzInputSocket when hardware buzz detected
     */
    static void onHardwareBuzz();
};

