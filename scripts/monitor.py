#!/usr/bin/env python3
"""Serial monitor for ESP32-C6 with optional reset suppression.

Prevents unexpected DTR/RTS resets when attaching to an already-running,
commissioned device.
"""

import argparse
import fcntl
import glob
import os
import select
import struct
import sys
import termios
import time

def find_serial_port():
    ports = glob.glob('/dev/cu.usbmodem*') + glob.glob('/dev/cu.usbserial*')
    if ports:
        return ports[0]
    return '/dev/cu.usbmodem14201'

def reset_chip(fd):
    print("Resetting ESP32-C6 into normal boot mode...")
    # DTR=0 (GPIO9 high -> normal flash boot)
    fcntl.ioctl(fd, termios.TIOCMBIC, struct.pack('I', termios.TIOCM_DTR))
    # RTS=1 (EN low -> reset asserted)
    fcntl.ioctl(fd, termios.TIOCMBIS, struct.pack('I', termios.TIOCM_RTS))
    time.sleep(0.1)
    # RTS=0 (EN high -> run)
    fcntl.ioctl(fd, termios.TIOCMBIC, struct.pack('I', termios.TIOCM_RTS))
    time.sleep(0.05)

def main():
    parser = argparse.ArgumentParser(description="ESP32 Serial Monitor")
    parser.add_argument("--port", "-p", default=None, help="Serial port (auto-detected if omitted)")
    parser.add_argument("--baud", "-b", type=int, default=115200, help="Baud rate (default: 115200)")
    parser.add_argument("--no-reset", action="store_true", help="Do not trigger DTR/RTS chip reset on connect")
def open_and_configure_port(port, baud):
    fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    attrs = termios.tcgetattr(fd)
    baud_const = getattr(termios, f"B{baud}", termios.B115200)
    attrs[4] = baud_const
    attrs[5] = baud_const
    # raw mode
    attrs[3] = attrs[3] & ~termios.ICANON & ~termios.ECHO
    termios.tcsetattr(fd, termios.TCSANOW, attrs)
    return fd

def main():
    parser = argparse.ArgumentParser(description="ESP32 Serial Monitor")
    parser.add_argument("--port", "-p", default=None, help="Serial port (auto-detected if omitted)")
    parser.add_argument("--baud", "-b", type=int, default=115200, help="Baud rate (default: 115200)")
    parser.add_argument("--no-reset", action="store_true", help="Do not trigger DTR/RTS chip reset on connect")
    parser.add_argument("--log-file", "-l", default="/tmp/intercom_serial.log", help="Path to append log output")
    args = parser.parse_args()

    port = args.port or find_serial_port()
    print(f"Connecting to {port} at {args.baud} baud (no-reset={args.no_reset})...")

    while True:
        try:
            fd = open_and_configure_port(port, args.baud)
            break
        except Exception as e:
            port = args.port or find_serial_port()
            time.sleep(0.5)

    if not args.no_reset:
        reset_chip(fd)

    print(f"Monitoring {port} -> {args.log_file} (Ctrl+C to stop)")
    with open(args.log_file, "a", buffering=1) as log_f:
        log_f.write(f"\n--- MONITOR STARTED AT {time.ctime()} ({port}) ---\n")
        line_buf = ""
        while True:
            try:
                r, _, _ = select.select([fd], [], [], 1.0)
                if r:
                    try:
                        chunk = os.read(fd, 1024)
                    except OSError as e:
                        chunk = b""
                    
                    if not chunk:
                        # Port disconnected (unplugged or rebooted via USB disconnect)
                        print(f"\n[Port disconnected, waiting for {port}...]", flush=True)
                        log_f.write(f"\n[Port disconnected at {time.ctime()}, waiting for {port}...]\n")
                        try:
                            os.close(fd)
                        except Exception:
                            pass
                        while True:
                            time.sleep(0.5)
                            try:
                                port = args.port or find_serial_port()
                                fd = open_and_configure_port(port, args.baud)
                                print(f"[Reconnected to {port}]", flush=True)
                                log_f.write(f"[Reconnected to {port} at {time.ctime()}]\n")
                                break
                            except Exception:
                                continue
                        continue

                    text = chunk.decode("utf-8", errors="replace")
                    line_buf += text
                    while "\n" in line_buf:
                        line, line_buf = line_buf.split("\n", 1)
                        line = line.rstrip("\r")
                        print(line, flush=True)
                        log_f.write(line + "\n")
                time.sleep(0.01)
            except KeyboardInterrupt:
                print("\nMonitoring stopped.")
                break
            except Exception as e:
                time.sleep(0.5)

if __name__ == '__main__':
    main()

