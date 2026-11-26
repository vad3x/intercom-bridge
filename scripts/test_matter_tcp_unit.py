import importlib.util
import json
import sys
import types
import unittest
from unittest.mock import MagicMock, patch
from pathlib import Path

# Discovery is mocked; zeroconf is not required to run these unit tests.
if importlib.util.find_spec("zeroconf") is None:
    zeroconf = types.ModuleType("zeroconf")
    for name in ("IPVersion", "ServiceBrowser", "ServiceInfo", "Zeroconf"):
        setattr(zeroconf, name, object)
    sys.modules["zeroconf"] = zeroconf

spec = importlib.util.spec_from_file_location("matter_tcp_smoke", Path(__file__).with_name("test_matter_tcp.py"))
smoke = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = smoke
spec.loader.exec_module(smoke)


class SmokeTest(unittest.TestCase):
    def test_requires_identification_without_explicit_ip(self):
        service = smoke.MatterService("other", ("192.0.2.1",), 5540, {"T": "4"})
        with patch.object(smoke, "is_intercom_bridge", return_value=False):
            with self.assertRaisesRegex(RuntimeError, "specify its address"):
                smoke.choose_bridge([service], None, 1)
            self.assertEqual(smoke.choose_bridge([service], "192.0.2.1", 1), (service, "192.0.2.1"))

    def test_returns_the_address_that_was_identified(self):
        service = smoke.MatterService("bridge", ("192.0.2.1", "192.0.2.2"), 5540, {"T": "4"})
        with patch.object(smoke, "is_intercom_bridge", side_effect=lambda ip, _: ip.endswith(".2")):
            self.assertEqual(smoke.choose_bridge([service], None, 1), (service, "192.0.2.2"))

    def test_reaccept_requires_new_log_for_peer_port(self):
        old = {"old Matter TCP accepted peer 192.0.2.10:1234"}
        with patch.object(smoke, "read_device_logs", return_value=old), \
             patch.object(smoke.time, "monotonic", side_effect=[0, 0, 2]), \
             patch.object(smoke.time, "sleep"):
            with self.assertRaisesRegex(RuntimeError, "did not log acceptance"):
                smoke.verify_accepted("192.0.2.20", 1234, old, 1)
        with patch.object(smoke, "read_device_logs", return_value=old | {"new Matter TCP accepted peer 192.0.2.10:1234"}):
            smoke.verify_accepted("192.0.2.20", 1234, old, 1)

    @staticmethod
    def _mock_http(payload: dict) -> MagicMock:
        response = MagicMock()
        response.status = 200
        response.read.return_value = json.dumps(payload).encode("utf-8")
        conn = MagicMock()
        conn.getresponse.return_value = response
        return conn

    def test_is_intercom_bridge_accepts_five_bucket_payload(self):
        payload = {
            "system": {"status": "Operational"},
            "device": {"type": "intercom_bridge", "state": {"door_locked": True}},
        }
        with patch.object(smoke.http.client, "HTTPConnection", return_value=self._mock_http(payload)):
            self.assertTrue(smoke.is_intercom_bridge("192.0.2.10", 1.0))

    def test_is_intercom_bridge_rejects_legacy_flat_and_invalid_payloads(self):
        legacy = {
            "device_type": "intercom_bridge",
            "door_locked": True,
            "active_slot": "ota_0",
            "mac": "AA:BB:CC:DD:EE:FF",
        }
        other_bridge = {
            "system": {"status": "Operational"},
            "device": {"type": "hvac_bridge", "state": {}},
        }
        missing_system_status = {
            "system": {},
            "device": {"type": "intercom_bridge", "state": {}},
        }
        for payload in (legacy, other_bridge, missing_system_status):
            with patch.object(smoke.http.client, "HTTPConnection", return_value=self._mock_http(payload)):
                self.assertFalse(smoke.is_intercom_bridge("192.0.2.10", 1.0))


if __name__ == "__main__":
    unittest.main()
