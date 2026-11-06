import importlib.util
import sys
import types
import unittest
from unittest.mock import patch
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


if __name__ == "__main__":
    unittest.main()
