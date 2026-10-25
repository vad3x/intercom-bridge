#!/usr/bin/env bash
set -euo pipefail

PORT="${1:-/dev/cu.usbmodem14201}"
BINARY="target/riscv32imac-unknown-none-elf/debug/intercom-bridge"

if [ ! -f "$BINARY" ]; then
    echo "Binary not found at $BINARY. Building first..."
    mise exec -- cargo build -p intercom-firmware
fi

echo "Flashing $BINARY to $PORT (erasing otadata to guarantee booting ota_0)..."
espflash flash "$BINARY" --port "$PORT" --partition-table partitions.csv --erase-parts otadata
