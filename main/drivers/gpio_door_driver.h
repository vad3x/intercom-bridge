/*
   Smart Intercom - GPIO Door Driver

   Hardware driver for door unlock relay control.
   Satisfies the DoorDriver concept.
*/

#pragma once

/**
 * @brief GPIO-based door unlock driver
 *
 * Controls PhotoMOS relay on UNLOCK_GPIO to release door latch.
 */
class GpioDoorDriver {
   public:
    /**
     * @brief Initialize the door unlock hardware
     */
    void init();

    /**
     * @brief Trigger a timed unlock pulse
     */
    void triggerUnlock();

    /**
     * @brief Check if unlock is currently in progress
     */
    bool isUnlocking() const;

    // Called when unlock pulse ends (from timer callback)
    void onUnlockComplete();

    // Static instance for timer callback access
    static GpioDoorDriver* instance_;

   private:
    volatile bool unlockInProgress_ = false;
};
