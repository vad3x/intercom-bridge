# BLE Advertising Fix After Commissioning

## Problem

After the Matter device was commissioned over BLE and connected to WiFi, the Rust implementation continued advertising as a BLE peripheral. The iPhone (Apple Home) detected this and cancelled the CASE session setup over WiFi with:

```
CHIP accessory server paired or being paired was unexpectedly discovered over BLE
```

This caused the post-commissioning secure channel (CASE) to fail, making the device unreachable from HomeKit.

The C++ (ESP-IDF) implementation stops BLE advertising automatically after commissioning completes, so this issue did not occur there.

## Root Cause

In `rs-matter-stack`, the `run_btp_coex` function ran BLE indefinitely — there was no mechanism to stop BLE advertising once the device was commissioned and connected to WiFi.

## Fix

**File:** `rs-matter-stack/src/wireless.rs` — `run_btp_coex`

The fix uses `embassy_futures::select` to race two futures:

1. **`btp_run`** — the BLE transport protocol (advertising + GATT service)
2. **`wait_netif_up`** — polls the network interface via `NetifDiag` until WiFi is operational with IPv6 addresses

When the WiFi network interface comes up, `wait_netif_up` completes first, which drops `btp_run` and stops BLE advertising. After that, the task stays alive via `core::future::pending()` so the network and mDNS tasks continue running.

```rust
if !self.matter().is_commissioned() {
    let btp_run = btp.run(
        "BT",
        self.matter().dev_det(),
        self.matter().dev_comm().discriminator,
    );

    let wait_netif_up = async {
        loop {
            let mut is_up = false;
            let _ = netif.netifs(&mut |ni| {
                if ni.operational && !ni.ipv6_addrs.is_empty() {
                    is_up = true;
                }
                Ok(())
            });
            if is_up {
                info!("Network interface up, stopping BLE advertising");
                break;
            }
            netif.wait_changed().await;
        }
    };

    // Run BTP until either it completes or the WiFi network comes up
    select(pin!(btp_run), pin!(wait_netif_up)).await;
} else {
    info!("Device already commissioned, skipping BLE advertising");
}

// After BLE stops, remain pending so net + mDNS keep running
core::future::pending::<Result<(), Error>>().await
```

### Why not `is_commissioned()`?

An earlier attempt stopped BLE as soon as `is_commissioned()` returned true (i.e., when a fabric was stored). This was **too early** — the iPhone still needed BLE to:

1. Receive the `AddNOC` response
2. Send WiFi network credentials via `NetworkCommissioning`

Stopping BLE at that point caused a "BLE ack recv timeout" and "Timeout on SendNOC" on the iPhone side.

Waiting for the WiFi interface to become operational is the correct signal — it means the device has already received WiFi credentials over BLE, connected to the network, and obtained IPv6 addresses. At that point BLE is no longer needed.

### Behavior

| Scenario | BLE Advertising |
|----------|----------------|
| Device not commissioned | Starts BLE, stops when WiFi interface comes up |
| Device already commissioned (reboot) | BLE never starts |
| BLE commissioning completes normally | BLE runs until WiFi is up, then stops |
