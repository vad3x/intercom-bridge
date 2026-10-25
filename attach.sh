#!/usr/bin/env bash
set -euo pipefail

PORT="${1:-/dev/cu.usbmodem14201}"
python3 scripts/monitor.py --port "$PORT" --no-reset
