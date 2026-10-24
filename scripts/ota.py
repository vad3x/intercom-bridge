#!/usr/bin/env python3
"""Smart Intercom Over-The-Air (OTA) Firmware Update Tool.

Discovers intercom devices on the local network via mDNS (_matter._tcp.local.),
signs firmware binaries with Ed25519 private key, and streams signed updates over HTTP.
"""

import argparse
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
import hashlib
import http.client
import json
import os
import re
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path
from typing import Dict, List, Optional, Tuple

# Graceful dependency imports
try:
    from cryptography.hazmat.primitives.asymmetric import ed25519
    from cryptography.hazmat.primitives import serialization
    HAVE_CRYPTO = True
except ImportError:
    HAVE_CRYPTO = False

try:
    import zeroconf
    HAVE_ZEROCONF = True
except ImportError:
    HAVE_ZEROCONF = False

MDNS_SERVICE_TYPES = ["_matter._tcp.local.", "_matterc._udp.local."]
MDNS_SERVICE_TYPE = MDNS_SERVICE_TYPES[0]
OTA_HEADER_MAGIC = b"ICOT"
OTA_HEADER_LEN = 72  # 4 (magic) + 4 (len) + 64 (sig)
MAX_APP_SLOT_SIZE = 0x350000  # 3.4 MB (3,473,408 bytes)
DEFAULT_KEY_PATH = Path.home() / ".intercom" / "ota_key.pem"


@dataclass
class DiscoveredDevice:
    name: str
    hostname: str
    ip: str
    port: int
    sw_ver: str
    slot: str
    mac: str

    def display_line(self) -> str:
        ver_info = f"v{self.sw_ver}" if self.sw_ver != "unknown" else "unknown ver"
        slot_info = f", running {self.slot}" if self.slot != "unknown" else ""
        host_info = f"{self.hostname} - " if self.hostname else ""
        return f"{self.name} ({host_info}{self.ip}:{self.port}) [{ver_info}{slot_info}]"


class OtaMdnsListener:
    """Zeroconf listener collecting Matter services concurrently."""

    def __init__(self, zc: "zeroconf.Zeroconf", executor: ThreadPoolExecutor):
        self.zc = zc
        self.executor = executor
        self.devices: Dict[str, "zeroconf.ServiceInfo"] = {}
        self.lock = threading.Lock()

    def add_service(self, zc: "zeroconf.Zeroconf", type_: str, name: str) -> None:
        self.executor.submit(self._resolve, type_, name)

    def update_service(self, zc: "zeroconf.Zeroconf", type_: str, name: str) -> None:
        self.executor.submit(self._resolve, type_, name)

    def remove_service(self, zc: "zeroconf.Zeroconf", type_: str, name: str) -> None:
        with self.lock:
            self.devices.pop(name, None)

    def _resolve(self, type_: str, name: str) -> None:
        try:
            info = self.zc.get_service_info(type_, name, timeout=1500)
            if info:
                with self.lock:
                    self.devices[name] = info
        except Exception:
            pass


def check_dependencies(require_zeroconf: bool = True) -> None:
    """Verify that required external dependencies are installed."""
    missing = []
    if not HAVE_CRYPTO:
        missing.append("cryptography")
    if require_zeroconf and not HAVE_ZEROCONF:
        missing.append("zeroconf")

    if missing:
        sys.stderr.write(
            f"Error: Missing required Python dependencies: {', '.join(missing)}\n"
            f"Please install them using:\n"
            f"    pip3 install {' '.join(missing)}\n"
        )
        sys.exit(1)


def probe_intercom_status(ip: str, port: int = 8080, timeout: float = 1.5) -> Optional[Dict]:
    """Probe an IP on port 8080 to check if it is an Intercom Bridge running the HTTP server."""
    try:
        conn = http.client.HTTPConnection(ip, port, timeout=timeout)
        conn.request("GET", "/api/status")
        res = conn.getresponse()
        if res.status == 200:
            data = json.loads(res.read().decode())
            if isinstance(data, dict):
                dev_type = data.get("device_type")
                if dev_type == "intercom_bridge":
                    return data
                if dev_type is None and "door_locked" in data and "active_slot" in data and "mac" in data:
                    return data
        conn.close()
    except Exception:
        pass
    return None


def discover_devices(timeout: float = 3.5, port: int = 8080) -> List[DiscoveredDevice]:
    """Scan the local network for intercom OTA services via mDNS."""
    check_dependencies(require_zeroconf=True)

    print("Scanning local network for Intercom units (Matter mDNS)...", flush=True)
    zc = zeroconf.Zeroconf()
    with ThreadPoolExecutor(max_workers=20) as resolver_pool:
        listener = OtaMdnsListener(zc, resolver_pool)
        browser = zeroconf.ServiceBrowser(zc, MDNS_SERVICE_TYPES, listener)

        try:
            time.sleep(timeout)
        finally:
            browser.cancel()

        # Shutdown resolver pool waiting for in-flight resolutions to finish
        resolver_pool.shutdown(wait=True)
        zc.close()

    with listener.lock:
        device_infos = list(listener.devices.values())

    # Collect unique candidate IPv4 addresses and associated hostnames
    candidates: Dict[str, str] = {}
    for info in device_infos:
        ipv4_addrs = [a for a in info.parsed_addresses() if re.match(r"^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}$", a)]
        hostname = (info.server or "").rstrip(".")
        for ip in ipv4_addrs:
            if ip not in candidates:
                candidates[ip] = hostname

    devices: List[DiscoveredDevice] = []

    def verify_candidate(item: Tuple[str, str]) -> Optional[DiscoveredDevice]:
        ip, hostname = item
        # Probe HTTP /api/status on port 8080 to verify device identity and read live telemetry
        status = probe_intercom_status(ip, port=port, timeout=1.5)
        if status:
            mac = status.get("mac") or "unknown"
            sw_ver = status.get("version") or "unknown"
            slot = status.get("active_slot") or "unknown"
            return DiscoveredDevice(
                name="Intercom Bridge",
                hostname=hostname,
                ip=ip,
                port=port,
                sw_ver=sw_ver,
                slot=slot,
                mac=mac,
            )
        return None

    if candidates:
        with ThreadPoolExecutor(max_workers=min(20, max(1, len(candidates)))) as executor:
            for dev in executor.map(verify_candidate, candidates.items()):
                if dev:
                    devices.append(dev)

    # Sort deterministically by name and IP
    devices.sort(key=lambda d: (d.name, d.ip))
    return devices


def parse_direct_target(target: str, default_port: int = 8080) -> Optional[DiscoveredDevice]:
    """If target is directly an IP or IP:PORT, parse it immediately."""
    ip_match = re.match(r"^(?P<ip>\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})(:(?P<port>\d+))?$", target.strip())
    if ip_match:
        ip = ip_match.group("ip")
        port = int(ip_match.group("port")) if ip_match.group("port") else default_port
        return DiscoveredDevice(
            name=f"Manual Target ({ip}:{port})",
            hostname="",
            ip=ip,
            port=port,
            sw_ver="unknown",
            slot="unknown",
            mac="unknown",
        )
    return None


def match_target(target: str, devices: List[DiscoveredDevice]) -> List[DiscoveredDevice]:
    """Match a target query string against discovered devices."""
    t_clean = target.strip().lower()
    t_mac = t_clean.replace(":", "").replace("-", "")

    matched = []
    for d in devices:
        d_mac = d.mac.lower().replace(":", "").replace("-", "")
        if (
            t_clean == d.name.lower()
            or t_clean in d.name.lower()
            or t_clean == d.hostname.lower()
            or t_clean == d.hostname.lower().rstrip(".local")
            or t_clean == d.ip
            or (t_mac and t_mac == d_mac)
        ):
            matched.append(d)

    return matched


def prompt_device_selection(devices: List[DiscoveredDevice]) -> List[DiscoveredDevice]:
    """Interactively ask user to select one or all devices."""
    print(f"Found {len(devices)} devices:")
    for idx, d in enumerate(devices, start=1):
        host_str = f"{d.hostname} - " if d.hostname else ""
        ver_info = f"v{d.sw_ver}" if d.sw_ver != "unknown" else "unknown ver"
        slot_info = f", running {d.slot}" if d.slot != "unknown" else ""
        print(f"  [{idx}] {d.name} ({host_str}{d.ip}) [{ver_info}{slot_info}]")
    print(f"  [A] All devices (sequential update)\n")

    while True:
        try:
            choice = input(f"Select target [1-{len(devices)}, A]: ").strip()
        except (EOFError, KeyboardInterrupt):
            print("\nAborted.")
            sys.exit(130)

        if choice.upper() == "A":
            return devices
        if choice.isdigit():
            idx = int(choice)
            if 1 <= idx <= len(devices):
                return [devices[idx - 1]]
        print(f"Invalid selection: '{choice}'. Enter 1-{len(devices)} or A.")


def sign_firmware(binary_data: bytes, key_path: Path) -> bytes:
    """Sign binary firmware with Ed25519 private key, returning 64-byte signature."""
    check_dependencies(require_zeroconf=False)

    if not key_path.exists():
        sys.stderr.write(
            f"Error: Signing key not found at {key_path}.\n"
            f"Please generate an Ed25519 key pair first using:\n"
            f"    python3 scripts/generate_keys.py\n"
        )
        sys.exit(1)

    try:
        with open(key_path, "rb") as f:
            private_key = serialization.load_pem_private_key(f.read(), password=None)
    except Exception as e:
        sys.stderr.write(f"Error reading private key from {key_path}: {e}\n")
        sys.exit(1)

    if not isinstance(private_key, ed25519.Ed25519PrivateKey):
        sys.stderr.write(f"Error: Key at {key_path} is not an Ed25519 private key.\n")
        sys.exit(1)

    return private_key.sign(binary_data)


def build_ota_header(binary_len: int, signature: bytes) -> bytes:
    """Build the 72-byte OTA packet header: [Magic 4B][Length u32 LE][Signature 64B]."""
    if len(signature) != 64:
        raise ValueError(f"Ed25519 signature must be exactly 64 bytes, got {len(signature)}")

    return OTA_HEADER_MAGIC + struct.pack("<I", binary_len) + signature


def render_progress(current: int, total: int, start_time: float, bar_width: int = 30) -> str:
    """Render terminal progress bar with transfer rate and percentage."""
    fraction = min(max(current / total, 0.0), 1.0) if total > 0 else 1.0
    percent = int(fraction * 100)
    filled_len = int(bar_width * fraction)
    bar = "█" * filled_len + "░" * (bar_width - filled_len)

    elapsed = time.time() - start_time
    speed = (current / elapsed) if elapsed > 0.05 else 0.0

    curr_mb = current / (1024 * 1024)
    total_mb = total / (1024 * 1024)
    speed_kb = speed / 1024
    if speed_kb >= 1024:
        speed_str = f"{speed_kb / 1024:.1f} MB/s"
    else:
        speed_str = f"{speed_kb:.0f} KB/s"

    return f"\r[{bar}] {percent:3d}% ({curr_mb:.2f}/{total_mb:.2f} MB) [{speed_str}]"


def find_espflash() -> Optional[str]:
    """Locate the espflash binary in PATH or standard cargo bin directory."""
    candidates = [
        "espflash",
        str(Path.home() / ".cargo" / "bin" / "espflash"),
    ]
    for c in candidates:
        found = shutil.which(c)
        if found:
            return found
    return None


def convert_elf_to_bin(elf_path: Path) -> Path:
    """Convert an ELF executable to an ESP32-C6 flat binary image using espflash."""
    espflash_cmd = find_espflash()
    if not espflash_cmd:
        sys.stderr.write(
            f"Error: '{elf_path.name}' is an ELF executable, but 'espflash' was not found in PATH.\n"
            f"espflash is required to convert ELF images to ESP32-C6 flat binaries.\n"
            f"Please install espflash using:\n"
            f"    cargo install espflash --locked\n"
            f"Or convert manually to a flat binary first:\n"
            f"    espflash save-image --chip esp32c6 --flash-size 8mb {elf_path} firmware.bin\n"
        )
        sys.exit(1)

    # Deterministic output path in tempdir based on ELF path and mtime
    mtime = elf_path.stat().st_mtime_ns
    path_hash = hashlib.sha256(f"{elf_path.resolve()}:{mtime}".encode()).hexdigest()[:12]
    out_path = Path(tempfile.gettempdir()) / f"ota_converted_{path_hash}.bin"

    cmd = [
        espflash_cmd,
        "save-image",
        "--chip",
        "esp32c6",
        "--flash-size",
        "8mb",
        str(elf_path),
        str(out_path),
    ]

    try:
        proc = subprocess.run(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
    except Exception as e:
        sys.stderr.write(f"Error executing espflash: {e}\n")
        sys.exit(1)

    if proc.returncode != 0:
        sys.stderr.write(
            f"Error: Failed to convert ELF '{elf_path.name}' using espflash:\n"
            f"{proc.stderr or proc.stdout}\n"
            f"You can manually convert using:\n"
            f"    {' '.join(cmd)}\n"
        )
        sys.exit(1)

    if not out_path.exists() or out_path.stat().st_size == 0:
        sys.stderr.write(f"Error: Converted binary not created at {out_path}\n")
        sys.exit(1)

    out_size = out_path.stat().st_size
    print(f"Detected ELF executable. Converting to ESP32-C6 flat binary via espflash ({out_size} bytes)...")
    return out_path


def push_ota_to_device(
    device: DiscoveredDevice,
    header: bytes,
    binary_path: Path,
    binary_len: int,
    http_timeout: float = 60.0,
) -> bool:
    """Stream signed firmware to target device over HTTP POST."""
    total_size = len(header) + binary_len
    target_desc = f"{device.name} ({device.ip}:{device.port})"

    print(f"\nTarget: {target_desc}")
    print(f"Firmware: {binary_path.name} ({binary_len / (1024*1024):.2f} MB, header: {len(header)} B)")

    start_time = time.time()
    conn = None
    try:
        conn = http.client.HTTPConnection(device.ip, device.port, timeout=http_timeout)
        conn.putrequest("POST", "/api/v1/ota/flash")
        conn.putheader("Content-Length", str(total_size))
        conn.putheader("Content-Type", "application/octet-stream")
        conn.endheaders()

        # Send header (72 bytes)
        conn.send(header)
        bytes_sent = len(header)

        # Stream binary payload
        chunk_size = 4096
        with open(binary_path, "rb") as f:
            while True:
                chunk = f.read(chunk_size)
                if not chunk:
                    break
                conn.send(chunk)
                bytes_sent += len(chunk)
                sys.stdout.write(render_progress(bytes_sent, total_size, start_time))
                sys.stdout.flush()

        print()  # End progress bar line
        print("Transfer complete. Awaiting device flash validation...")

        resp = conn.getresponse()
        resp_data = resp.read().decode("utf-8", errors="replace").strip()

        if resp.status == 200:
            elapsed = time.time() - start_time
            print(f"✅ SUCCESS ({elapsed:.1f}s): Device verified signature and initiated reboot.")
            if resp_data:
                print(f"   Device message: {resp_data}")
            return True
        else:
            print(f"❌ FAILED: HTTP {resp.status} {resp.reason}")
            if resp_data:
                print(f"   Device message: {resp_data}")
            return False

    except BrokenPipeError as e:
        print()
        # Attempt to read pending HTTP response from device before reporting error
        server_responded = False
        if conn:
            try:
                resp = conn.getresponse()
                resp_data = resp.read().decode("utf-8", errors="replace").strip()
                if resp.status == 200:
                    elapsed = time.time() - start_time
                    print(f"✅ SUCCESS ({elapsed:.1f}s): Device verified signature and initiated reboot.")
                    if resp_data:
                        print(f"   Device message: {resp_data}")
                    return True
                print(f"❌ Device rejected update: HTTP {resp.status} {resp.reason}")
                if resp_data:
                    print(f"   Device message: {resp_data}")
                server_responded = True
            except Exception:
                pass
        if not server_responded:
            print(f"❌ Connection broken communicating with {target_desc}: {e}")
        return False
    except (http.client.HTTPException, OSError) as e:
        print()
        server_responded = False
        if conn:
            try:
                resp = conn.getresponse()
                resp_data = resp.read().decode("utf-8", errors="replace").strip()
                if resp.status == 200:
                    elapsed = time.time() - start_time
                    print(f"✅ SUCCESS ({elapsed:.1f}s): Device verified signature and initiated reboot.")
                    if resp_data:
                        print(f"   Device message: {resp_data}")
                    return True
                print(f"❌ Device rejected update: HTTP {resp.status} {resp.reason}")
                if resp_data:
                    print(f"   Device message: {resp_data}")
                server_responded = True
            except Exception:
                pass
        if not server_responded:
            print(f"❌ Connection error communicating with {target_desc}: {e}")
        return False
    finally:
        if conn:
            try:
                conn.close()
            except Exception:
                pass


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Smart Intercom Over-The-Air (OTA) Firmware Updater",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""Examples:
  # Discover devices on network and flash interactively:
  python3 scripts/ota.py target/riscv32imac-unknown-none-elf/release/intercom-bridge.bin

  # Flash directly to specific IP:
  python3 scripts/ota.py --target 192.168.1.142 firmware.bin

  # Flash directly by device name (matches mDNS announcement):
  python3 scripts/ota.py --target "Front Door" firmware.bin
""",
    )
    parser.add_argument("firmware", help="Path to compiled firmware binary (.bin or ELF executable)")
    parser.add_argument(
        "--target",
        "-t",
        default=None,
        help="Direct target IP address, hostname, device name, or MAC (bypasses prompt)",
    )
    parser.add_argument(
        "--port",
        "-p",
        type=int,
        default=8080,
        help="Target HTTP OTA port (default: 8080)",
    )
    parser.add_argument(
        "--key",
        "-k",
        default=str(DEFAULT_KEY_PATH),
        help=f"Path to Ed25519 private key PEM file (default: {DEFAULT_KEY_PATH})",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=3.5,
        help="mDNS discovery timeout in seconds (default: 3.5)",
    )
    parser.add_argument(
        "--http-timeout",
        type=float,
        default=60.0,
        help="HTTP upload timeout in seconds (default: 60.0)",
    )

    args = parser.parse_args()

    # 1. Validate firmware binary path and size
    binary_path = Path(args.firmware).resolve()
    if not binary_path.exists() or not binary_path.is_file():
        sys.stderr.write(f"Error: Firmware binary not found: {binary_path}\n")
        sys.exit(1)

    # Check if input file is an ELF executable and auto-convert to flat binary if needed
    with open(binary_path, "rb") as f:
        magic = f.read(4)

    if magic == b"\x7fELF":
        binary_path = convert_elf_to_bin(binary_path)

    binary_len = binary_path.stat().st_size
    if binary_len == 0:
        sys.stderr.write(f"Error: Firmware binary is empty: {binary_path}\n")
        sys.exit(1)

    if binary_len > MAX_APP_SLOT_SIZE:
        sys.stderr.write(
            f"Error: Firmware binary size ({binary_len} bytes / {binary_len/(1024*1024):.2f} MB) "
            f"exceeds maximum OTA partition capacity ({MAX_APP_SLOT_SIZE} bytes / 3.4 MB).\n"
        )
        sys.exit(1)

    # 2. Sign binary
    key_path = Path(args.key).expanduser().resolve()
    print(f"Reading firmware: {binary_path.name} ({binary_len} bytes)")
    print(f"Signing with Ed25519 key: {key_path}")
    binary_data = binary_path.read_bytes()
    signature = sign_firmware(binary_data, key_path)
    header = build_ota_header(binary_len, signature)
    print(f"Generated 72-byte OTA header (Magic: ICOT, Signature: {signature[:8].hex()}...)")

    # 3. Resolve target devices
    targets: List[DiscoveredDevice] = []

    if args.target:
        # Check if direct IP was specified
        direct_dev = parse_direct_target(args.target, default_port=args.port)
        if direct_dev:
            targets = [direct_dev]
        else:
            # Query mDNS and match against target string
            discovered = discover_devices(timeout=args.timeout, port=args.port)
            matches = match_target(args.target, discovered)
            if len(matches) == 1:
                targets = matches
            elif len(matches) > 1:
                print(f"Ambiguous target '{args.target}' matched {len(matches)} devices.")
                targets = prompt_device_selection(matches)
            else:
                # If target looks like a hostname (e.g. intercom-84fc.local), try resolving it
                hostname = args.target.rstrip(".")
                try:
                    resolved_ip = socket.gethostbyname(hostname)
                    status = probe_intercom_status(resolved_ip, port=args.port, timeout=1.0)
                    targets = [
                        DiscoveredDevice(
                            name="Intercom Bridge" if status else f"Hostname Target ({hostname})",
                            hostname=hostname,
                            ip=resolved_ip,
                            port=args.port,
                            sw_ver=status.get("version") if status else "unknown",
                            slot=status.get("active_slot") if status else "unknown",
                            mac=status.get("mac") if status else "unknown",
                        )
                    ]
                except socket.gaierror:
                    sys.stderr.write(
                        f"Error: Target '{args.target}' not found in mDNS discovery or DNS resolution.\n"
                    )
                    sys.exit(1)
    else:
        # Auto-discovery
        discovered = discover_devices(timeout=args.timeout, port=args.port)
        if not discovered:
            sys.stderr.write(
                "No Intercom OTA devices found on local network.\n"
                "Ensure devices are powered on and on the same Wi-Fi subnet, or specify --target <ip>.\n"
            )
            sys.exit(1)
        elif len(discovered) == 1:
            dev = discovered[0]
            print(f"Found 1 device:\n  {dev.display_line()}")
            targets = [dev]
        else:
            targets = prompt_device_selection(discovered)

    # 4. Push OTA update to target(s)
    success_count = 0
    total_targets = len(targets)
    for idx, target in enumerate(targets, start=1):
        if total_targets > 1:
            print(f"\n--- Updating Device {idx}/{total_targets} ---")
        if push_ota_to_device(target, header, binary_path, binary_len, http_timeout=args.http_timeout):
            success_count += 1

    if total_targets > 1:
        print(f"\nSummary: {success_count}/{total_targets} devices updated successfully.")

    if success_count < total_targets:
        sys.exit(1)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\nAborted by user.")
        sys.exit(130)
