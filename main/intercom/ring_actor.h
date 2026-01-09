/*
   Smart Intercom - Ring Actor

   Listens to hardware buzz detection and emits RingPressed events.
*/

#pragma once

#include "hardware_sockets.h"

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
     * @param socket Pointer to BuzzInputSocket implementation
     */
    void init(BuzzInputSocket* socket);

private:
    /**
     * @brief Called by BuzzInputSocket when hardware buzz detected
     */
    static void onHardwareBuzz();
};

