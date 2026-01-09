/*
   Smart Intercom - GPIO Adapter

   Implements hardware socket interfaces for ESP32 GPIO.
*/

#pragma once

#include "hardware/hardware_sockets.h"

/**
 * @brief GPIO adapter implementing hardware sockets
 *
 * Handles:
 * - LockOutputSocket: PhotoMOS relay control for door unlock
 * - BuzzInputSocket: Optocoupler input for buzz detection
 */
class GpioAdapter : public LockOutputSocket, public BuzzInputSocket {
public:
    // LockOutputSocket implementation
    void triggerUnlock() override;
    bool isUnlocking() const override;

    // BuzzInputSocket implementation
    void init(Callback onBuzz) override;

    // Called from ISR context after debounce
    static void onBuzzDetected();

    // Called when unlock pulse ends (from timer callback)
    void onUnlockComplete();

    // Static instance for ISR access
    static GpioAdapter* instance_;

private:
    Callback buzzCallback_;
    volatile bool unlockInProgress_ = false;
};

