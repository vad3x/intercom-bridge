#!/bin/bash
# Run Smart Intercom firmware in QEMU ESP32-C6 emulator

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="$PROJECT_DIR/build"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}=========================================${NC}"
echo -e "${GREEN}  Smart Intercom QEMU Launcher${NC}"
echo -e "${GREEN}=========================================${NC}"

# Check if firmware is built
if [ ! -f "$BUILD_DIR/intercom-matter.bin" ]; then
    echo -e "${RED}Error: Firmware not found at $BUILD_DIR/intercom-matter.bin${NC}"
    echo ""
    echo "Please build the firmware first:"
    echo "  1. Open project in Dev Container, OR"
    echo "  2. Run: docker-compose --profile build up build"
    echo ""
    exit 1
fi

echo -e "${YELLOW}Firmware found: $BUILD_DIR/intercom-matter.bin${NC}"
echo ""
echo "Starting QEMU ESP32-C6 emulator..."
echo "  - HTTP server will be available at: http://localhost:8080"
echo "  - QEMU monitor on port 5555"
echo "  - Press Ctrl+A, X to exit QEMU"
echo ""

# Start QEMU via Docker Compose
cd "$SCRIPT_DIR"
docker-compose up qemu-esp32c6

echo ""
echo -e "${GREEN}QEMU stopped.${NC}"



