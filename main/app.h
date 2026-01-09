/*
   Smart Intercom - Application Class

   Encapsulates all actors, drivers, and dependencies.
*/

#pragma once

#include <esp_err.h>
#include "drivers/gpio_door_driver.h"
#include "drivers/gpio_ring_driver.h"
#include "drivers/matter_access_control_driver.h"
#include "events/events.h"

// Include actor headers (templates)
#include "actors/access_control_actor.h"
#include "actors/door_actor.h"
#include "actors/ring_actor.h"

class App {
   public:
    explicit App();

    esp_err_t init();

   private:
    // Event bus
    EventBus eventBus_;

    // Hardware drivers
    GpioRingDriver ringDriver_;
    GpioDoorDriver doorDriver_;
    MatterAccessControlDriver accessControlDriver_;

    // Actors (templates instantiated with concrete drivers)
    RingActor<GpioRingDriver> ringActor_;
    AccessControlActor<MatterAccessControlDriver> accessControlActor_;
    DoorActor<GpioDoorDriver> doorActor_;
};

// Get the singleton app instance
App& getApp();
