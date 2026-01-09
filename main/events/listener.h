/*
   Smart Intercom - Event Listener Template

   Generic event listener with multiple subscribers.
   Fixed-size array to avoid heap allocation (embedded-friendly).
*/

#pragma once

template<typename TEvent, int MaxSubscribers = 4>
class Listener {
public:
    using Handler = void (*)(const TEvent&);

    /**
     * @brief Subscribe a handler to this event
     * @param handler Function pointer to call when event is emitted
     * @return true if subscribed, false if full or null handler
     */
    bool subscribe(Handler handler) {
        if (count_ < MaxSubscribers && handler) {
            handlers_[count_++] = handler;
            return true;
        }
        return false;
    }

    /**
     * @brief Emit event to all subscribers
     * @param event The event to emit
     */
    void emit(const TEvent& event) {
        for (int i = 0; i < count_; i++) {
            handlers_[i](event);
        }
    }

    /**
     * @brief Get subscriber count
     * @return Number of subscribed handlers
     */
    int count() const { return count_; }

private:
    Handler handlers_[MaxSubscribers] = {};
    int count_ = 0;
};

