/*
   Smart Intercom - Hardware Socket Interfaces

   Abstract interfaces for hardware interaction (ports in hexagonal architecture).
   Implemented by GpioAdapter.
*/

#pragma once

#include <functional>

/**
 * @brief Output port for lock hardware (relay control)
 *
 * Implemented by GpioAdapter to control the PhotoMOS relay.
 */
class LockOutputSocket {
   public:
    virtual ~LockOutputSocket() = default;

    /**
     * @brief Trigger unlock pulse on the relay
     */
    virtual void triggerUnlock() = 0;

    /**
     * @brief Check if unlock pulse is currently in progress
     * @return true if relay is currently activated
     */
    virtual bool isUnlocking() const = 0;
};

/**
 * @brief Input port for buzz detection hardware (optocoupler)
 *
 * Implemented by GpioAdapter to detect intercom ring signals.
 */
class BuzzInputSocket {
   public:
    using Callback = std::function<void()>;

    virtual ~BuzzInputSocket() = default;

    /**
     * @brief Initialize buzz detection with callback
     * @param onBuzz Callback to invoke when buzz is detected
     */
    virtual void init(Callback onBuzz) = 0;
};
