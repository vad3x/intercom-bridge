/*
   Smart Intercom - Ring Actor Implementation
*/

#include "ring_actor.h"
#include "events.h"
#include <esp_log.h>

static const char* TAG = "RingActor";

void RingActor::init(BuzzInputSocket* socket) {
    ESP_LOGI(TAG, "Initializing RingActor");

    if (socket) {
        socket->init(&RingActor::onHardwareBuzz);
        ESP_LOGI(TAG, "BuzzInputSocket connected");
    } else {
        ESP_LOGW(TAG, "No BuzzInputSocket provided");
    }
}

void RingActor::onHardwareBuzz() {
    ESP_LOGI(TAG, "Hardware buzz detected - emitting RingPressed");
    g_eventBus.emitRingPressed("RingActor");
}

