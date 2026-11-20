#!/usr/bin/env python3
"""Deterministic WebRTC Talk -> loopback.wav diagnostic test using aiortc.

Run with:
    uv run --with aiortc --with av --with numpy python3 scripts/test_webrtc_talk_loopback.py --ip 192.168.0.180
"""

from __future__ import annotations

import argparse
import asyncio
import fractions
import io
import math
import time
import urllib.request
import wave

import av
import numpy as np
from aiortc import MediaStreamTrack, RTCPeerConnection, RTCSessionDescription


class SineWaveAudioTrack(MediaStreamTrack):
    """Generates a continuous 440 Hz 48 kHz mono PCM sine wave in 20 ms frames."""

    kind = "audio"

    def __init__(self, freq_hz: float = 440.0, duration_sec: float = 2.0) -> None:
        super().__init__()
        self.freq_hz = freq_hz
        self.sample_rate = 48_000
        self.samples_per_frame = 960  # 20 ms @ 48 kHz
        self.total_frames = int(duration_sec * 50)
        self.frame_index = 0
        self._start_time: float | None = None

    async def recv(self) -> av.AudioFrame:
        if self._start_time is None:
            self._start_time = time.monotonic()

        target = self._start_time + self.frame_index * 0.020
        now = time.monotonic()
        if target > now:
            await asyncio.sleep(target - now)

        t0 = self.frame_index * self.samples_per_frame
        idx = np.arange(t0, t0 + self.samples_per_frame, dtype=np.float64)
        pcm = (12_000.0 * np.sin(2.0 * math.pi * self.freq_hz * idx / self.sample_rate)).astype(
            np.int16
        )

        frame = av.AudioFrame.from_ndarray(pcm.reshape(1, -1), format="s16", layout="mono")
        frame.sample_rate = self.sample_rate
        frame.pts = self.frame_index * self.samples_per_frame
        frame.time_base = fractions.Fraction(1, self.sample_rate)
        self.frame_index += 1
        return frame


def http_post_sdp(url: str, sdp: str) -> str:
    req = urllib.request.Request(
        url,
        data=sdp.encode("utf-8"),
        headers={"Content-Type": "application/sdp"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=8) as resp:
            return resp.read().decode("utf-8")
    except urllib.error.HTTPError as err:
        body = err.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"POST {url} (sdp_len={len(sdp)}) -> HTTP {err.code}: {body}") from err


def http_post_empty(url: str) -> None:
    req = urllib.request.Request(url, data=b"", method="POST")
    with urllib.request.urlopen(req, timeout=5):
        pass


def analyze_wav(wav_bytes: bytes, expected_sec: float) -> int:
    with wave.open(io.BytesIO(wav_bytes), "rb") as wf:
        rate = wf.getframerate()
        channels = wf.getnchannels()
        width = wf.getsampwidth()
        nframes = wf.getnframes()
        raw = wf.readframes(nframes)

    samples = np.frombuffer(raw, dtype=np.int16).astype(np.float64)
    duration_ms = (nframes * 1000.0) / rate
    frame_size = 320  # 20 ms @ 16 kHz
    num_20ms = len(samples) // frame_size

    silent_frames = []
    for i in range(num_20ms):
        chunk = samples[i * frame_size : (i + 1) * frame_size]
        rms = float(np.sqrt(np.mean(chunk**2)))
        if rms < 200.0:
            silent_frames.append((i, round(rms, 1)))

    # Check interior (excluding first/last 320 samples ramp) for step glitches.
    interior = samples[frame_size : max(frame_size, len(samples) - frame_size)]
    diffs = np.abs(np.diff(interior)) if len(interior) > 1 else np.array([0.0])
    max_step = float(np.max(diffs))
    # Expected max sample-to-sample step for 440 Hz @ 16 kHz amp 12000 is ~2070.
    glitch_indices = np.where(diffs > 4500.0)[0]

    rms_total = float(np.sqrt(np.mean(samples**2))) if len(samples) else 0.0
    print(
        f"WAV: rate={rate} ch={channels} bits={width*8} frames_20ms={num_20ms} "
        f"duration_ms={duration_ms:.0f} (expected ~{expected_sec*1000:.0f})"
    )
    print(f"Stats: rms={rms_total:.1f} max_step={max_step:.1f} glitches(>4500)={len(glitch_indices)}")
    if silent_frames:
        print(f"Silent/dropout 20ms frames ({len(silent_frames)}): {silent_frames[:15]}")
    if len(glitch_indices) > 0:
        print(f"Glitch sample offsets: {glitch_indices[:15].tolist()}")
    return 0 if (num_20ms >= int(expected_sec * 40) and len(glitch_indices) == 0 and len(silent_frames) <= 2) else 1


async def run_test(ip: str, port: int, duration_sec: float, out_path: str | None) -> int:
    base = f"http://{ip}:{port}"
    pc = RTCPeerConnection()
    track = SineWaveAudioTrack(freq_hz=440.0, duration_sec=duration_sec)
    transceiver = pc.addTransceiver("audio", direction="sendrecv")
    transceiver.sender.replaceTrack(track)

    offer = await pc.createOffer()
    await pc.setLocalDescription(offer)
    # Wait briefly for host ICE candidate gathering.
    for _ in range(40):
        if pc.iceGatheringState == "complete":
            break
        await asyncio.sleep(0.05)

    offer_sdp = "\r\n".join(
        line
        for line in pc.localDescription.sdp.splitlines()
        if not line.startswith(("a=fingerprint:sha-384", "a=fingerprint:sha-512"))
    ) + "\r\n"
    print(f"Posting SDP offer to {base}/api/debug/audio/webrtc/offer...")
    answer_sdp = http_post_sdp(f"{base}/api/debug/audio/webrtc/offer", offer_sdp)
    await pc.setRemoteDescription(RTCSessionDescription(sdp=answer_sdp, type="answer"))

    for _ in range(100):
        if pc.connectionState == "connected":
            break
        await asyncio.sleep(0.05)
    print(f"WebRTC state: {pc.connectionState}")
    if pc.connectionState != "connected":
        await pc.close()
        http_post_empty(f"{base}/api/debug/audio/webrtc/stop")
        return 2

    print(f"Streaming 440 Hz sine wave for {duration_sec:.1f}s...")
    await asyncio.sleep(duration_sec + 0.25)

    await pc.close()
    http_post_empty(f"{base}/api/debug/audio/webrtc/stop")

    # Wait past BENCH_LOOPBACK_RELEASE_GAP_MS (500 ms).
    await asyncio.sleep(0.8)

    wav_url = f"{base}/api/debug/audio/loopback.wav"
    print(f"Downloading {wav_url}...")
    with urllib.request.urlopen(wav_url, timeout=8) as resp:
        wav_bytes = resp.read()

    if out_path:
        with open(out_path, "wb") as f:
            f.write(wav_bytes)
        print(f"Saved {len(wav_bytes)} bytes to {out_path}")

    return analyze_wav(wav_bytes, duration_sec)


def main() -> int:
    parser = argparse.ArgumentParser(description="Test WebRTC Talk -> loopback.wav end-to-end.")
    parser.add_argument("--ip", default="192.168.0.180")
    parser.add_argument("--port", type=int, default=8080)
    parser.add_argument("--duration", type=float, default=2.0)
    parser.add_argument("--out", default="/tmp/webrtc_loopback_test.wav")
    args = parser.parse_args()
    return asyncio.run(run_test(args.ip, args.port, args.duration, args.out))


if __name__ == "__main__":
    raise SystemExit(main())
