/*
   Smart Intercom - Application Class

   Encapsulates all actors, drivers, and dependencies.
*/

#pragma once

#include <esp_err.h>
#include "config/app_config.h"
#include "drivers/gpio_door_driver.h"
#include "drivers/gpio_ring_driver.h"
#include "drivers/matter_access_control_driver.h"
#include "events/events.h"

// Include actor headers (templates)
#include "actors/access_control_actor.h"
#include "actors/door_actor.h"
#include "actors/ring_actor.h"
#include "actors/indicator_actor.h"

// Indicator driver - conditionally include the right implementation
#if LED_ENABLED
#include "drivers/gpio_led_driver.h"
using IndicatorDriverImpl = GpioLedDriver;
#else
#include "drivers/stub_led_driver.h"
using IndicatorDriverImpl = StubLedDriver;
#endif

// Verify implementation satisfies the concept at compile time
static_assert(IndicatorDriver<IndicatorDriverImpl>, "IndicatorDriverImpl must satisfy IndicatorDriver concept");

class App {
   public:
    explicit App();

    esp_err_t init();

    // Access to event bus for emitting events
    EventBus& eventBus() { return eventBus_; }

   private:
    // Indicator driver (created first)
    IndicatorDriverImpl indicatorDriver_;

    // Event bus
    EventBus eventBus_;

    // Hardware drivers
    GpioRingDriver ringDriver_;
    GpioDoorDriver doorDriver_;
    MatterAccessControlDriver accessControlDriver_;

    // Actors (templates instantiated with concrete drivers)
    IndicatorActor<IndicatorDriverImpl> indicatorActor_;
    RingActor<GpioRingDriver> ringActor_;
    AccessControlActor<MatterAccessControlDriver> accessControlActor_;
    DoorActor<GpioDoorDriver> doorActor_;
};

// Get the singleton app instance
App& getApp();
