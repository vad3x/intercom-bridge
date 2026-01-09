/*
   Smart Intercom - Application Class

   Encapsulates all actors and dependencies.
*/

#pragma once

#include <esp_err.h>
#include "events/events.h"
#include "hardware/gpio_adapter.h"
#include "actors/ring_actor.h"
#include "actors/intercom_actor.h"
#include "actors/lock_actor.h"
#include "actors/matter_actor.h"

class App {
public:
    App();

    esp_err_t init();

    // Access to MatterActor for SDK callbacks
    MatterActor& matterActor() { return matterActor_; }

private:
    EventBus eventBus_;
    GpioAdapter gpioAdapter_;
    RingActor ringActor_;
    IntercomActor intercomActor_;
    LockActor lockActor_;
    MatterActor matterActor_;
};

// Global app instance (for Matter SDK callbacks)
extern App g_app;

