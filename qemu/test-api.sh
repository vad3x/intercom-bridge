#!/bin/bash
# Test Smart Intercom HTTP API
# Run this while QEMU is running

set -e

BASE_URL="${INTERCOM_URL:-http://localhost:8080}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

echo -e "${CYAN}=========================================${NC}"
echo -e "${CYAN}  Smart Intercom API Test${NC}"
echo -e "${CYAN}=========================================${NC}"
echo ""
echo "Target: $BASE_URL"
echo ""

# Test 1: GET /status
echo -e "${YELLOW}Test 1: GET /status${NC}"
echo "---"
STATUS_RESPONSE=$(curl -s "$BASE_URL/status" || echo '{"error": "Connection failed"}')
echo "$STATUS_RESPONSE" | python3 -m json.tool 2>/dev/null || echo "$STATUS_RESPONSE"
echo ""

# Check if status was successful
if echo "$STATUS_RESPONSE" | grep -q '"device"'; then
    echo -e "${GREEN}✓ Status endpoint working${NC}"
else
    echo -e "${RED}✗ Status endpoint failed${NC}"
fi
echo ""

# Test 2: POST /unlock
echo -e "${YELLOW}Test 2: POST /unlock${NC}"
echo "---"
UNLOCK_RESPONSE=$(curl -s -X POST "$BASE_URL/unlock" || echo '{"error": "Connection failed"}')
echo "$UNLOCK_RESPONSE" | python3 -m json.tool 2>/dev/null || echo "$UNLOCK_RESPONSE"
echo ""

# Check if unlock was successful
if echo "$UNLOCK_RESPONSE" | grep -q '"success": true'; then
    echo -e "${GREEN}✓ Unlock triggered successfully${NC}"
elif echo "$UNLOCK_RESPONSE" | grep -q '"success": false'; then
    echo -e "${YELLOW}! Unlock returned false (lockout may be active)${NC}"
else
    echo -e "${RED}✗ Unlock endpoint failed${NC}"
fi
echo ""

# Test 3: Rapid unlock (should fail due to lockout)
echo -e "${YELLOW}Test 3: POST /unlock (immediate retry - should fail)${NC}"
echo "---"
UNLOCK_RETRY=$(curl -s -X POST "$BASE_URL/unlock" || echo '{"error": "Connection failed"}')
echo "$UNLOCK_RETRY" | python3 -m json.tool 2>/dev/null || echo "$UNLOCK_RETRY"
echo ""

if echo "$UNLOCK_RETRY" | grep -q '"success": false'; then
    echo -e "${GREEN}✓ Lockout working correctly${NC}"
else
    echo -e "${YELLOW}! Unexpected response (lockout may not be active)${NC}"
fi
echo ""

# Test 4: Final status check
echo -e "${YELLOW}Test 4: GET /status (verify counts)${NC}"
echo "---"
FINAL_STATUS=$(curl -s "$BASE_URL/status" || echo '{"error": "Connection failed"}')
echo "$FINAL_STATUS" | python3 -m json.tool 2>/dev/null || echo "$FINAL_STATUS"
echo ""

echo -e "${CYAN}=========================================${NC}"
echo -e "${CYAN}  Tests Complete${NC}"
echo -e "${CYAN}=========================================${NC}"



