/*
   Smart Intercom - Ring Actor

   Listens to hardware buzz detection and emits RingPressed events.
*/

#pragma once

#include "hardware/hardware_sockets.h"

struct EventBus;  // Forward declaration

/**
 * @brief Ring detection actor
 *
 * Connects BuzzInputSocket to the event bus.
 * When hardware detects a buzz, emits RingPressed event.
 */
class RingActor {
   public:
    RingActor(EventBus& bus, BuzzInputSocket& socket);

    void init();

   private:
    static EventBus* bus_;
    static BuzzInputSocket* socket_;

    /**
     * @brief Called by BuzzInputSocket when hardware buzz detected
     */
    static void onHardwareBuzz();
};
