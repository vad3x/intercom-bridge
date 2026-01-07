#!/usr/bin/env python3
"""
GPIO Simulator for Smart Intercom QEMU Testing

Since QEMU cannot fully emulate the optocoupler/PhotoMOS hardware,
this script provides a way to simulate GPIO events via the QEMU monitor.

Usage:
    python3 gpio-sim.py buzz     # Simulate doorbell ring
    python3 gpio-sim.py status   # Check GPIO status
    python3 gpio-sim.py help     # Show help
"""

import socket
import sys
import time

QEMU_MONITOR_HOST = "localhost"
QEMU_MONITOR_PORT = 5555

def connect_monitor():
    """Connect to QEMU monitor."""
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.connect((QEMU_MONITOR_HOST, QEMU_MONITOR_PORT))
        sock.settimeout(2.0)
        # Read initial prompt
        try:
            sock.recv(1024)
        except socket.timeout:
            pass
        return sock
    except ConnectionRefusedError:
        print(f"Error: Cannot connect to QEMU monitor at {QEMU_MONITOR_HOST}:{QEMU_MONITOR_PORT}")
        print("Make sure QEMU is running with monitor enabled.")
        sys.exit(1)

def send_command(sock, cmd):
    """Send command to QEMU monitor and get response."""
    sock.send((cmd + "\n").encode())
    time.sleep(0.1)
    try:
        response = sock.recv(4096).decode()
        return response
    except socket.timeout:
        return ""

def simulate_buzz():
    """Simulate a doorbell buzz by toggling GPIO."""
    print("Simulating doorbell buzz...")
    print("")
    print("Note: GPIO simulation in QEMU is limited.")
    print("The firmware will detect this as a GPIO interrupt if supported.")
    print("")
    
    sock = connect_monitor()
    
    # In a real QEMU setup with GPIO support, we would use:
    # qemu_set_irq or similar mechanisms
    # For now, we demonstrate the concept
    
    print("Sending simulated buzz signal...")
    
    # This is a placeholder - actual GPIO manipulation depends on
    # QEMU's ESP32-C6 implementation
    response = send_command(sock, "info registers")
    print("QEMU response:", response[:200] if response else "(no response)")
    
    sock.close()
    
    print("")
    print("To test buzz detection without QEMU GPIO support:")
    print("  1. Check logs in QEMU terminal for 'BUZZ DETECTED' messages")
    print("  2. Use the HTTP API: curl http://localhost:8080/status")
    print("")

def show_status():
    """Show QEMU status."""
    print("Connecting to QEMU monitor...")
    
    sock = connect_monitor()
    
    response = send_command(sock, "info status")
    print("QEMU Status:", response.strip() if response else "Running")
    
    response = send_command(sock, "info version")
    print("QEMU Version:", response.strip() if response else "Unknown")
    
    sock.close()

def show_help():
    """Show help message."""
    print(__doc__)
    print("")
    print("Commands:")
    print("  buzz    - Simulate doorbell ring (GPIO event)")
    print("  status  - Show QEMU emulator status")
    print("  help    - Show this help message")
    print("")
    print("Configuration:")
    print(f"  QEMU Monitor: {QEMU_MONITOR_HOST}:{QEMU_MONITOR_PORT}")
    print("")
    print("For HTTP API testing, use test-api.sh instead.")

def main():
    if len(sys.argv) < 2:
        show_help()
        sys.exit(0)
    
    cmd = sys.argv[1].lower()
    
    if cmd == "buzz":
        simulate_buzz()
    elif cmd == "status":
        show_status()
    elif cmd == "help" or cmd == "-h" or cmd == "--help":
        show_help()
    else:
        print(f"Unknown command: {cmd}")
        print("Use 'help' for available commands.")
        sys.exit(1)

if __name__ == "__main__":
    main()



