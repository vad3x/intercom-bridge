#!/usr/bin/env python3
"""Smoke-test Matter-over-TCP discovery, framing, and reconnect on an Intercom Bridge."""

from __future__ import annotations

import argparse
import http.client
import json
import socket
import struct
import sys
import threading
import time
from dataclasses import dataclass
from typing import Any

try:
    from zeroconf import IPVersion, ServiceBrowser, ServiceInfo, Zeroconf
except ImportError as error:
    raise SystemExit("Missing Python dependency: install it with `python3 -m pip install zeroconf`") from error

SERVICE_TYPE = "_matter._tcp.local."
MATTER_PORT = 5540
TEST_PAYLOAD = b"matter-tcp-smoke"


@dataclass(frozen=True)
class MatterService:
    name: str
    addresses: tuple[str, ...]
    port: int
    txt: dict[str, str]


class ServiceCollector:
    """Resolve Matter TCP service announcements while the browser is running."""

    def __init__(self, zeroconf: Zeroconf) -> None:
        self.zeroconf = zeroconf
        self.lock = threading.Lock()
        self.services: dict[str, ServiceInfo] = {}

    def add_service(self, _zc: Zeroconf, service_type: str, name: str) -> None:
        self._resolve(service_type, name)

    def update_service(self, _zc: Zeroconf, service_type: str, name: str) -> None:
        self._resolve(service_type, name)

    def remove_service(self, _zc: Zeroconf, _service_type: str, name: str) -> None:
        with self.lock:
            self.services.pop(name, None)

    def _resolve(self, service_type: str, name: str) -> None:
        try:
            info = self.zeroconf.get_service_info(service_type, name, timeout=1500)
        except Exception:
            return
        if info is not None:
            with self.lock:
                self.services[name] = info

    def snapshot(self) -> list[ServiceInfo]:
        with self.lock:
            return list(self.services.values())


def read_txt(info: ServiceInfo) -> dict[str, str]:
    values: dict[str, str] = {}
    for key, value in info.properties.items():
        key_text = key.decode("utf-8", errors="replace") if isinstance(key, bytes) else str(key)
        value_text = value.decode("utf-8", errors="replace") if isinstance(value, bytes) else str(value)
        values[key_text] = value_text
    return values


def discover_services(timeout: float) -> list[MatterService]:
    zeroconf = Zeroconf(ip_version=IPVersion.All)
    collector = ServiceCollector(zeroconf)
    browser = ServiceBrowser(zeroconf, SERVICE_TYPE, collector)
    deadline = time.monotonic() + timeout
    try:
        while time.monotonic() < deadline:
            time.sleep(0.1)
        time.sleep(0.4)  # allow final SRV/TXT/A/AAAA responses to finish resolving
        return [
            MatterService(
                info.name,
                tuple(info.parsed_addresses(IPVersion.All)),
                info.port,
                read_txt(info),
            )
            for info in collector.snapshot()
        ]
    finally:
        browser.cancel()
        zeroconf.close()


def is_intercom_bridge(ip: str, timeout: float) -> bool:
    """Use the bridge status endpoint to avoid choosing an unrelated Matter node."""
    connection: http.client.HTTPConnection | None = None
    try:
        connection = http.client.HTTPConnection(ip, 8080, timeout=timeout)
        connection.request("GET", "/api/status")
        response = connection.getresponse()
        if response.status != 200:
            return False
        data: Any = json.loads(response.read().decode("utf-8"))
        return isinstance(data, dict) and (
            data.get("device_type") == "intercom_bridge"
            or ("door_locked" in data and "active_slot" in data and "mac" in data)
        )
    except Exception:
        return False
    finally:
        if connection is not None:
            connection.close()


def advertises_tcp_server(txt: dict[str, str]) -> bool:
    """Matter T is a bitmap: value 4 is the TCP server bit (T=6 also includes client)."""
    try:
        return bool(int(txt.get("T", "0")) & 4)
    except ValueError:
        return False


def choose_bridge(services: list[MatterService], preferred_ip: str | None, timeout: float) -> MatterService:
    candidates = [service for service in services if service.port == MATTER_PORT]
    if preferred_ip:
        candidates = [service for service in candidates if preferred_ip in service.addresses]
    if not candidates:
        raise RuntimeError(f"No {SERVICE_TYPE} service on port {MATTER_PORT} was found")

    supported = [service for service in candidates if advertises_tcp_server(service.txt)]
    for service in supported:
        for ip in service.addresses:
            if is_intercom_bridge(ip, timeout):
                return service
    # Some firmware profiles do not expose the HTTP API; retain mDNS as a fallback.
    if supported:
        return supported[0]
    raise RuntimeError("Matter TCP service found, but no record advertised the TCP server bit in T")


def connect(ip: str, timeout: float) -> socket.socket:
    connection = socket.create_connection((ip, MATTER_PORT), timeout=timeout)
    connection.settimeout(timeout)
    return connection


def verify_socket_stays_open(connection: socket.socket) -> None:
    # The deliberately non-Matter smoke payload is expected to be discarded without
    # closing the TCP session. No application reply is expected.
    connection.settimeout(0.7)
    try:
        data = connection.recv(1)
    except (TimeoutError, socket.timeout):
        return
    if data == b"":
        raise RuntimeError("Device closed the TCP connection after the framed smoke message")


def run(timeout: float, preferred_ip: str | None) -> None:
    checks: list[tuple[str, str]] = []
    services = discover_services(timeout)
    if not services:
        raise RuntimeError(f"No services found for {SERVICE_TYPE}")

    service = choose_bridge(services, preferred_ip, min(timeout, 2.0))
    tcp_flag = service.txt.get("T")
    if not advertises_tcp_server(service.txt):
        raise RuntimeError(
            f"{service.name} TXT record has T={tcp_flag!r}, expected the TCP server bit (4)"
        )
    addresses = [address for address in service.addresses if ":" not in address]
    if preferred_ip:
        addresses = [address for address in addresses if address == preferred_ip]
    if not addresses:
        raise RuntimeError(f"{service.name} has no usable IPv4 address: {service.addresses}")
    ip = addresses[0]
    checks.append(("mDNS TXT", f"{service.name} at {ip}, T={tcp_flag} (TCP server bit set)"))

    first = connect(ip, timeout)
    checks.append(("TCP handshake", f"connected to {ip}:{MATTER_PORT}"))
    try:
        frame = struct.pack("<I", len(TEST_PAYLOAD)) + TEST_PAYLOAD
        first.sendall(frame)
        verify_socket_stays_open(first)
        checks.append(("Framed message", f"sent {len(frame)} bytes; connection remained open"))
    finally:
        try:
            first.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        first.close()

    time.sleep(0.2)
    second = connect(ip, timeout)
    try:
        checks.append(("Reconnect", f"second TCP handshake to {ip}:{MATTER_PORT} succeeded"))
    finally:
        second.close()

    for label, detail in checks:
        print(f"PASS {label}: {detail}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--timeout", type=float, default=4.0, help="mDNS and socket timeout in seconds")
    parser.add_argument("--ip", help="limit discovery to this device IP")
    args = parser.parse_args()
    try:
        run(args.timeout, args.ip)
    except Exception as error:
        print(f"FAIL Matter-over-TCP verification: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
