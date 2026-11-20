"""Constants for the Intercom Bridge integration."""

DOMAIN = "intercom_bridge"

CONF_HOST = "host"
CONF_PORT = "port"
DEFAULT_PORT = 8080

STATIC_CARD_URL = "/intercom_bridge/intercom-bridge-card.js"
WS_PROXY_PATH = "/api/intercom_bridge/ws/{entry_id}"
STATUS_PROXY_PATH = "/api/intercom_bridge/status/{entry_id}"
