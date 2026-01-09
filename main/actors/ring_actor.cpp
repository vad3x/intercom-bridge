/*
   Smart Intercom - Ring Actor Implementation
*/

#include "actors/ring_actor.h"
#include "events/events.h"
#include <esp_log.h>

static const char* TAG = "RingActor";

// Static member definitions
EventBus* RingActor::bus_ = nullptr;
BuzzInputSocket* RingActor::socket_ = nullptr;

RingActor::RingActor(EventBus& bus, BuzzInputSocket& socket) {
    bus_ = &bus;
    socket_ = &socket;
}

void RingActor::init() {
    ESP_LOGI(TAG, "Initializing RingActor");
    socket_->init(&RingActor::onHardwareBuzz);
    ESP_LOGI(TAG, "BuzzInputSocket connected");
}

void RingActor::onHardwareBuzz() {
    ESP_LOGI(TAG, "Hardware buzz detected - emitting RingPressed");
    if (bus_) {
        bus_->emitRingPressed("RingActor");
    }
}

