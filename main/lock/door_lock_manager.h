/*
   Smart Intercom - Simplified Door Lock Manager

   Minimal implementation for intercom door release:
   - No PIN codes, users, or schedules
   - Lock command is no-op (door is always locked by spring)
   - Unlock command triggers GPIO pulse to release door
*/

#pragma once

#include <app/clusters/door-lock-server/door-lock-server.h>
#include <lib/core/DataModelTypes.h>

using chip::ByteSpan;
using chip::EndpointId;
using chip::Optional;
using chip::app::Clusters::DoorLock::DlLockState;
using chip::app::Clusters::DoorLock::OperationErrorEnum;

/**
 * @brief Simplified bolt lock manager for intercom
 *
 * This manager handles the basic lock/unlock operations without
 * any user management, credentials, or schedules.
 */
class BoltLockManager {
   public:
    /**
     * @brief Initialize the lock manager
     * @return CHIP_NO_ERROR on success
     */
    CHIP_ERROR Init();

    /**
     * @brief Initialize lock state from Matter cluster
     * Called after Matter stack is initialized
     * @return CHIP_NO_ERROR on success
     */
    CHIP_ERROR InitLockState();

    /**
     * @brief Handle lock command (no-op for intercom)
     * The door is always locked by its spring mechanism.
     * This just updates the Matter state.
     *
     * @param endpointId The endpoint to lock
     * @param pin Optional PIN (ignored - no PIN support)
     * @param err Error output
     * @return true on success
     */
    bool Lock(EndpointId endpointId, const Optional<ByteSpan> &pin, OperationErrorEnum &err);

    /**
     * @brief Handle unlock command - triggers relay pulse
     * This is the main action - pulses the PhotoMOS relay to release the door.
     *
     * @param endpointId The endpoint to unlock
     * @param pin Optional PIN (ignored - no PIN support)
     * @param err Error output
     * @return true on success
     */
    bool Unlock(EndpointId endpointId, const Optional<ByteSpan> &pin, OperationErrorEnum &err);

    /**
     * @brief Get singleton instance
     */
    static BoltLockManager &GetInstance() {
        return sInstance;
    }

   private:
    static BoltLockManager sInstance;

    /**
     * @brief Set lock state and optionally trigger hardware
     *
     * @param endpointId Endpoint to update
     * @param lockState New lock state
     * @param triggerHardware If true, pulse the relay on unlock
     * @return true on success
     */
    bool SetLockState(EndpointId endpointId, DlLockState lockState, bool triggerHardware);

    /**
     * @brief Convert lock state to string for logging
     */
    const char *LockStateToString(DlLockState state) const;
};

/**
 * @brief Convenience macro to get lock manager instance
 */
#define BoltLockMgr() BoltLockManager::GetInstance()
