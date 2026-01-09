/*
   Smart Intercom - Ring Actor Implementation
*/

#include "actors/ring_actor.h"
#include "events/events.h"
#include <esp_log.h>

static const char* TAG = "RingActor";

// Static member definition
EventBus* RingActor::bus_ = nullptr;

void RingActor::init(EventBus& bus, BuzzInputSocket* socket) {
    ESP_LOGI(TAG, "Initializing RingActor");

    bus_ = &bus;

    if (socket) {
        socket->init(&RingActor::onHardwareBuzz);
        ESP_LOGI(TAG, "BuzzInputSocket connected");
    } else {
        ESP_LOGW(TAG, "No BuzzInputSocket provided");
    }
}

void RingActor::onHardwareBuzz() {
    ESP_LOGI(TAG, "Hardware buzz detected - emitting RingPressed");
    if (bus_) {
        bus_->emitRingPressed("RingActor");
    }
}

