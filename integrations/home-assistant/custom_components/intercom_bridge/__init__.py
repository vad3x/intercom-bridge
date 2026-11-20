"""Intercom Bridge Home Assistant integration."""

from __future__ import annotations

import asyncio
import logging
from pathlib import Path
from typing import Any

import aiohttp
from aiohttp import WSMsgType, web

from homeassistant.components.frontend import add_extra_js_url
from homeassistant.components.http import HomeAssistantView
from homeassistant.config_entries import ConfigEntry
from homeassistant.core import HomeAssistant
from homeassistant.helpers.aiohttp_client import async_get_clientsession
from homeassistant.helpers.typing import ConfigType

from .const import (
    CONF_HOST,
    CONF_PORT,
    DEFAULT_PORT,
    DOMAIN,
    STATIC_CARD_URL,
    STATUS_PROXY_PATH,
    WS_PROXY_PATH,
)

_LOGGER = logging.getLogger(__name__)
CARD_FILE = Path(__file__).parent / "www" / "intercom-bridge-card.js"


async def async_setup(hass: HomeAssistant, config: ConfigType) -> bool:
    """Set up shared HTTP views and Lovelace card static asset."""
    hass.data.setdefault(DOMAIN, {})

    if CARD_FILE.exists():
        try:
            from homeassistant.components.http import StaticPathConfig

            await hass.http.async_register_static_paths(
                [StaticPathConfig(STATIC_CARD_URL, str(CARD_FILE), cache_headers=False)]
            )
        except ImportError:
            hass.http.register_static_path(STATIC_CARD_URL, str(CARD_FILE), cache_headers=False)

        add_extra_js_url(hass, STATIC_CARD_URL)

    hass.http.register_view(IntercomEntriesView(hass))
    hass.http.register_view(IntercomStatusProxyView(hass))
    hass.http.register_view(IntercomAudioWsProxyView(hass))
    return True


async def async_setup_entry(hass: HomeAssistant, entry: ConfigEntry) -> bool:
    """Store Intercom Bridge connection details for a config entry."""
    hass.data.setdefault(DOMAIN, {})
    hass.data[DOMAIN][entry.entry_id] = {
        CONF_HOST: entry.data[CONF_HOST],
        CONF_PORT: int(entry.data.get(CONF_PORT, DEFAULT_PORT)),
        "title": entry.title,
    }
    return True


async def async_unload_entry(hass: HomeAssistant, entry: ConfigEntry) -> bool:
    """Unload a config entry."""
    hass.data.get(DOMAIN, {}).pop(entry.entry_id, None)
    return True


class IntercomEntriesView(HomeAssistantView):
    """Return configured Intercom Bridge entries for card auto-discovery."""

    url = "/api/intercom_bridge/entries"
    name = "api:intercom_bridge:entries"
    requires_auth = True

    def __init__(self, hass: HomeAssistant) -> None:
        self._hass = hass

    async def get(self, request: web.Request) -> web.Response:
        entries = [
            {
                "entry_id": entry_id,
                "title": info["title"],
                "host": info[CONF_HOST],
                "port": info[CONF_PORT],
            }
            for entry_id, info in self._hass.data.get(DOMAIN, {}).items()
            if isinstance(info, dict) and CONF_HOST in info
        ]
        return self.json({"entries": entries})


class IntercomStatusProxyView(HomeAssistantView):
    """Proxy GET /api/status from the ESP32 Intercom Bridge."""

    url = STATUS_PROXY_PATH
    name = "api:intercom_bridge:status"
    requires_auth = True

    def __init__(self, hass: HomeAssistant) -> None:
        self._hass = hass

    async def get(self, request: web.Request, entry_id: str) -> web.Response:
        target = self._hass.data.get(DOMAIN, {}).get(entry_id)
        if not target:
            return web.json_response({"error": "unknown entry_id"}, status=404)

        session = async_get_clientsession(self._hass)
        url = f"http://{target[CONF_HOST]}:{target[CONF_PORT]}/api/status"
        try:
            async with asyncio.timeout(5):
                async with session.get(url) as resp:
                    body = await resp.text()
                    return web.Response(
                        text=body,
                        status=resp.status,
                        content_type="application/json",
                    )
        except (aiohttp.ClientError, TimeoutError) as err:
            return web.json_response({"error": str(err)}, status=502)


class IntercomAudioWsProxyView(HomeAssistantView):
    """Authenticated WSS-to-WS proxy for /api/ws/audio on the ESP32 Intercom Bridge."""

    url = WS_PROXY_PATH
    name = "api:intercom_bridge:ws"
    # Browser WebSocket API cannot set Authorization headers; validate ?token= below.
    requires_auth = False

    def __init__(self, hass: HomeAssistant) -> None:
        self._hass = hass

    async def get(self, request: web.Request, entry_id: str) -> web.StreamResponse:
        token = request.query.get("token", "")
        if not token or self._hass.auth.async_validate_access_token(token) is None:
            return web.Response(status=401, text="Unauthorized")

        target = self._hass.data.get(DOMAIN, {}).get(entry_id)
        if not target:
            return web.Response(status=404, text="Unknown Intercom Bridge entry")

        client_ws = web.WebSocketResponse(heartbeat=20.0)
        await client_ws.prepare(request)

        device_url = f"ws://{target[CONF_HOST]}:{target[CONF_PORT]}/api/ws/audio"
        session = async_get_clientsession(self._hass)

        try:
            async with session.ws_connect(device_url, heartbeat=20.0) as device_ws:
                await _pump_bidirectional(client_ws, device_ws)
        except (aiohttp.ClientError, TimeoutError) as err:
            _LOGGER.warning("Intercom WS proxy error for %s: %s", device_url, err)
            if not client_ws.closed:
                await client_ws.close(code=1011, message=b"Device connection failed")
        return client_ws


async def _pump_bidirectional(
    client_ws: web.WebSocketResponse,
    device_ws: aiohttp.ClientWebSocketResponse,
) -> None:
    """Forward binary/text WebSocket frames between browser and ESP32."""

    async def forward(src: Any, dst: Any) -> None:
        async for msg in src:
            if msg.type == WSMsgType.BINARY:
                await dst.send_bytes(msg.data)
            elif msg.type == WSMsgType.TEXT:
                await dst.send_str(msg.data)
            elif msg.type in (WSMsgType.CLOSE, WSMsgType.CLOSING, WSMsgType.CLOSED):
                break
            elif msg.type == WSMsgType.ERROR:
                break

    tasks = [
        asyncio.create_task(forward(client_ws, device_ws)),
        asyncio.create_task(forward(device_ws, client_ws)),
    ]
    done, pending = await asyncio.wait(tasks, return_when=asyncio.FIRST_COMPLETED)
    for task in pending:
        task.cancel()
    for task in done:
        exc = task.exception()
        if exc:
            _LOGGER.debug("Intercom WS pump ended with exception: %s", exc)

    if not client_ws.closed:
        code = device_ws.close_code or 1000
        await client_ws.close(code=code)
    if not device_ws.closed:
        await device_ws.close()
