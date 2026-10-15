#!/usr/bin/env python3
"""Generate an Ed25519 key pair for Smart Intercom OTA firmware signing."""

import sys
from pathlib import Path

try:
    from cryptography.hazmat.primitives.asymmetric import ed25519
    from cryptography.hazmat.primitives import serialization
except ImportError:
    sys.stderr.write(
        "Error: 'cryptography' library is not installed.\n"
        "Please install it using: pip3 install cryptography\n"
    )
    sys.exit(1)

KEY_DIR = Path.home() / ".intercom"
KEY_PATH = KEY_DIR / "ota_key.pem"


def main():
    KEY_DIR.mkdir(parents=True, exist_ok=True)
    if KEY_PATH.exists():
        print(f"Key already exists at {KEY_PATH}")
        with open(KEY_PATH, "rb") as f:
            private_key = serialization.load_pem_private_key(f.read(), password=None)
    else:
        private_key = ed25519.Ed25519PrivateKey.generate()
        pem = private_key.private_bytes(
            encoding=serialization.Encoding.PEM,
            format=serialization.PrivateFormat.PKCS8,
            encryption_algorithm=serialization.NoEncryption(),
        )
        with open(KEY_PATH, "wb") as f:
            f.write(pem)
        KEY_PATH.chmod(0o600)
        print(f"Generated new Ed25519 private key: {KEY_PATH}")

    public_key = private_key.public_key()
    pub_bytes = public_key.public_bytes(
        encoding=serialization.Encoding.Raw,
        format=serialization.PublicFormat.Raw,
    )
    rust_bytes = ", ".join(f"0x{b:02x}" for b in pub_bytes)
    print("\nCopy this public key into your firmware config:")
    print(f"pub const OTA_PUBLIC_KEY: [u8; 32] = [{rust_bytes}];\n")


if __name__ == "__main__":
    main()
