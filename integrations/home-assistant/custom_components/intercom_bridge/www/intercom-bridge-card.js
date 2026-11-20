const SAMPLE_RATE = 16000;
const FRAME_SAMPLES = 320;
const CTRL_HEARTBEAT = 0x00;
const CTRL_START_TALK = 0x01;
const CTRL_STOP_TALK = 0x02;
const MAX_BUFFERED_BYTES = FRAME_SAMPLES * 3;
const MAX_PLAYBACK_SECONDS = 0.12;
const TARGET_PLAYOUT_LEAD_SECONDS = 0.06;

const ULAW_BIAS = 0x84;
const ULAW_CLIP = 32635;

function encodeMulawSample(pcm16) {
  const sign = pcm16 < 0 ? 0x80 : 0x00;
  let mag = Math.abs(pcm16);
  if (mag > ULAW_CLIP) mag = ULAW_CLIP;
  mag += ULAW_BIAS;
  let exponent = 7;
  for (let expMask = 0x4000; (mag & expMask) === 0 && exponent > 0; exponent -= 1) {
    expMask >>= 1;
  }
  const mantissa = (mag >> (exponent + 3)) & 0x0f;
  return ~(sign | (exponent << 4) | mantissa) & 0xff;
}

function decodeMulawSample(ulaw) {
  const u = ~ulaw & 0xff;
  const sign = (u & 0x80) !== 0;
  const exponent = (u >> 4) & 0x07;
  const mantissa = u & 0x0f;
  const magnitude = (((mantissa << 3) + ULAW_BIAS) << exponent) - ULAW_BIAS;
  return sign ? -magnitude : magnitude;
}

function decodeMulawToFloat32(bytes) {
  const out = new Float32Array(bytes.length);
  for (let i = 0; i < bytes.length; i += 1) {
    out[i] = decodeMulawSample(bytes[i]) / 32768;
  }
  return out;
}

function encodeFloat32ToMulaw(samples) {
  const out = new Uint8Array(samples.length);
  for (let i = 0; i < samples.length; i += 1) {
    const clamped = Math.max(-1, Math.min(1, samples[i]));
    out[i] = encodeMulawSample(Math.round(clamped * 32767));
  }
  return out;
}

class IntercomBridgeCard extends HTMLElement {
  static getStubConfig() {
    return {
      title: "Apartment Intercom",
      lock_entity: "",
      ring_entity: "",
    };
  }

  constructor() {
    super();
    this.attachShadow({ mode: "open" });
    this._config = {};
    this._hass = null;
    this._entryId = null;
    this._connected = false;
    this._talking = false;
    this._talkRequested = false;
    this._talkGeneration = 0;
    this._error = "";
    this._ws = null;
    this._audioCtx = null;
    this._mediaStream = null;
    this._source = null;
    this._processor = null;
    this._heartbeatTimer = null;
    this._playback = new Set();
    this._nextPlayTime = 0;
    this._unlocking = false;
    this._rendered = false;
  }

  setConfig(config) {
    this._config = {
      title: config.title || "Intercom Bridge",
      entry_id: config.entry_id || "",
      lock_entity: config.lock_entity || "",
      ring_entity: config.ring_entity || "",
    };
    if (this._config.entry_id) {
      this._entryId = this._config.entry_id;
    }
    this._render();
  }

  set hass(hass) {
    const firstSet = !this._hass;
    this._hass = hass;
    if (firstSet && !this._entryId) {
      this._resolveEntryId();
    }
    this._updateStateUI();
  }

  getCardSize() {
    return 3;
  }

  disconnectedCallback() {
    this._disconnect();
  }

  async _resolveEntryId() {
    if (!this._hass || this._config.entry_id) return;
    try {
      const res = await this._hass.callApi("GET", "intercom_bridge/entries");
      if (res?.entries?.length > 0) {
        this._entryId = res.entries[0].entry_id;
        this._error = "";
        this._updateStateUI();
      } else {
        this._error = "No Intercom Bridge integration entry configured.";
        this._updateStateUI();
      }
    } catch (err) {
      this._error = `Failed to load Intercom Bridge entry: ${err.message || err}`;
      this._updateStateUI();
    }
  }

  async _connect() {
    if (this._audioCtx || !this._hass) return;
    if (!this._entryId) {
      await this._resolveEntryId();
      if (!this._entryId) return;
    }

    const token = this._hass?.auth?.data?.access_token;
    if (!token) {
      this._error = "Missing Home Assistant auth token.";
      this._updateStateUI();
      return;
    }

    this._error = "";
    const context = new AudioContext({ sampleRate: SAMPLE_RATE });
    this._audioCtx = context;
    try {
      await context.resume();
      if (this._audioCtx !== context) return;
      this._nextPlayTime = context.currentTime;

      const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
      const url = `${proto}//${window.location.host}/api/intercom_bridge/ws/${encodeURIComponent(
        this._entryId
      )}?token=${encodeURIComponent(token)}`;

      const ws = new WebSocket(url);
      ws.binaryType = "arraybuffer";
      this._ws = ws;

      ws.onopen = () => {
        if (this._ws !== ws) return;
        this._connected = true;
        this._heartbeatTimer = setInterval(() => this._sendControl(CTRL_HEARTBEAT), 2000);
        this._updateStateUI();
      };

      ws.onmessage = (event) => {
        if (this._ws !== ws || this._talking || !(event.data instanceof ArrayBuffer)) return;
        if (event.data.byteLength !== FRAME_SAMPLES) return;
        this._playFrame(context, new Uint8Array(event.data));
      };

      ws.onclose = (event) => {
        if (this._ws !== ws) return;
        if (event.code === 1013) {
          this._error =
            !event.reason || event.reason === "preempted"
              ? "Audio session preempted by another caller."
              : `Audio session unavailable: ${event.reason}`;
        }
        this._cleanup();
      };

      ws.onerror = () => {
        if (this._ws !== ws) return;
        this._error = "Audio WebSocket connection failed.";
        this._disconnect();
      };
    } catch (err) {
      if (this._audioCtx === context) this._cleanup();
      this._error = err?.message || String(err);
      this._updateStateUI();
    }
  }

  _disconnect() {
    this._stopTalk();
    if (this._ws) {
      try {
        this._ws.close();
      } catch (_) {}
    }
    this._cleanup();
  }

  _cleanup() {
    this._talkGeneration += 1;
    this._talkRequested = false;
    this._talking = false;
    this._connected = false;
    if (this._heartbeatTimer) {
      clearInterval(this._heartbeatTimer);
      this._heartbeatTimer = null;
    }
    this._stopMic();
    this._clearPlayback();
    if (this._audioCtx) {
      this._audioCtx.close().catch(() => {});
      this._audioCtx = null;
    }
    this._ws = null;
    this._updateStateUI();
  }

  async _startTalk() {
    if (!this._connected) {
      await this._connect();
    }
    const ws = this._ws;
    const context = this._audioCtx;
    if (!ws || ws.readyState !== WebSocket.OPEN || !context || this._talkRequested) return;

    this._talkRequested = true;
    const generation = ++this._talkGeneration;

    try {
      if (!navigator.mediaDevices?.getUserMedia) {
        throw new Error("Microphone requires HTTPS (or localhost) and browser permission.");
      }
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: { echoCancellation: true, noiseSuppression: true, channelCount: 1 },
      });
      if (this._ws !== ws || generation !== this._talkGeneration || !this._talkRequested) {
        for (const track of stream.getTracks()) track.stop();
        return;
      }

      this._mediaStream = stream;
      this._source = context.createMediaStreamSource(stream);
      const processor = context.createScriptProcessor(512, 1, 1);
      this._processor = processor;
      const frame = new Float32Array(FRAME_SAMPLES);
      let filled = 0;

      processor.onaudioprocess = (event) => {
        if (!this._talking || this._ws !== ws || ws.readyState !== WebSocket.OPEN) return;
        const input = event.inputBuffer.getChannelData(0);
        for (let i = 0; i < input.length; i += 1) {
          frame[filled++] = input[i];
          if (filled === FRAME_SAMPLES) {
            if (ws.bufferedAmount < MAX_BUFFERED_BYTES) {
              ws.send(encodeFloat32ToMulaw(frame));
            }
            filled = 0;
          }
        }
      };

      this._source.connect(processor);
      processor.connect(context.destination);
      this._clearPlayback();
      this._sendControl(CTRL_START_TALK);
      this._talking = true;
      this._error = "";
      this._updateStateUI();
    } catch (err) {
      if (generation !== this._talkGeneration) return;
      this._error = err?.message || String(err);
      this._stopTalk();
    }
  }

  _stopTalk() {
    this._talkGeneration += 1;
    this._talkRequested = false;
    const wasTalking = this._talking;
    this._talking = false;
    this._stopMic();
    if (wasTalking) {
      this._sendControl(CTRL_STOP_TALK);
    }
    this._updateStateUI();
  }

  _stopMic() {
    if (this._processor) {
      this._processor.onaudioprocess = null;
      try {
        this._processor.disconnect();
      } catch (_) {}
      this._processor = null;
    }
    if (this._source) {
      try {
        this._source.disconnect();
      } catch (_) {}
      this._source = null;
    }
    if (this._mediaStream) {
      for (const track of this._mediaStream.getTracks()) track.stop();
      this._mediaStream = null;
    }
  }

  _sendControl(code) {
    if (this._ws && this._ws.readyState === WebSocket.OPEN) {
      this._ws.send(new Uint8Array([code]));
    }
  }

  _playFrame(context, mulawBytes) {
    const pcm = decodeMulawToFloat32(mulawBytes);
    const buffer = context.createBuffer(1, pcm.length, SAMPLE_RATE);
    buffer.copyToChannel(pcm, 0);
    const source = context.createBufferSource();
    source.buffer = buffer;
    source.connect(context.destination);

    const now = context.currentTime;
    if (this._nextPlayTime < now) {
      this._nextPlayTime = now + TARGET_PLAYOUT_LEAD_SECONDS;
    } else if (this._nextPlayTime - now > MAX_PLAYBACK_SECONDS) {
      this._nextPlayTime = now + TARGET_PLAYOUT_LEAD_SECONDS;
    }

    source.onended = () => this._playback.delete(source);
    this._playback.add(source);
    source.start(this._nextPlayTime);
    this._nextPlayTime += buffer.duration;
  }

  _clearPlayback() {
    for (const source of this._playback) {
      try {
        source.stop();
      } catch (_) {}
    }
    this._playback.clear();
    if (this._audioCtx) {
      this._nextPlayTime = this._audioCtx.currentTime;
    }
  }

  async _unlockDoor() {
    if (!this._hass || !this._config.lock_entity || this._unlocking) return;
    this._unlocking = true;
    this._updateStateUI();
    try {
      await this._hass.callService("lock", "unlock", {
        entity_id: this._config.lock_entity,
      });
    } catch (err) {
      this._error = `Unlock failed: ${err?.message || err}`;
    } finally {
      this._unlocking = false;
      this._updateStateUI();
    }
  }

  _render() {
    if (this._rendered) {
      this._updateStateUI();
      return;
    }
    this._rendered = true;
    this.shadowRoot.innerHTML = `
      <style>
        ha-card {
          padding: 16px;
        }
        .header {
          display: flex;
          align-items: center;
          justify-content: space-between;
          margin-bottom: 14px;
        }
        .title {
          font-size: 1.15rem;
          font-weight: 600;
          color: var(--primary-text-color);
        }
        .badge {
          font-size: 0.78rem;
          padding: 3px 9px;
          border-radius: 999px;
          background: var(--secondary-background-color);
          color: var(--secondary-text-color);
          font-weight: 500;
        }
        .badge.live {
          background: rgba(var(--rgb-primary-color, 3, 169, 244), 0.18);
          color: var(--primary-color);
        }
        .badge.talking {
          background: rgba(244, 67, 54, 0.18);
          color: var(--error-color, #db4437);
        }
        .controls {
          display: grid;
          grid-template-columns: repeat(auto-fit, minmax(110px, 1fr));
          gap: 10px;
        }
        button {
          appearance: none;
          border: 1px solid var(--divider-color, rgba(0, 0, 0, 0.12));
          border-radius: 10px;
          padding: 12px 14px;
          font-size: 0.95rem;
          font-weight: 600;
          cursor: pointer;
          background: var(--secondary-background-color);
          color: var(--primary-text-color);
          transition: transform 0.08s ease, background 0.15s ease;
          user-select: none;
          -webkit-user-select: none;
          touch-action: none;
        }
        button:active {
          transform: scale(0.98);
        }
        button.active {
          background: var(--primary-color);
          color: var(--text-primary-color, #fff);
          border-color: transparent;
        }
        button.talking {
          background: var(--error-color, #db4437);
          color: #fff;
          border-color: transparent;
        }
        button.unlock {
          background: rgba(76, 175, 80, 0.15);
          color: var(--success-color, #2e7d32);
          border-color: rgba(76, 175, 80, 0.35);
        }
        button:disabled {
          opacity: 0.5;
          cursor: not-allowed;
        }
        .meta {
          margin-top: 10px;
          font-size: 0.82rem;
          color: var(--secondary-text-color);
        }
        .error {
          margin-top: 10px;
          font-size: 0.85rem;
          color: var(--error-color, #db4437);
        }
      </style>
      <ha-card>
        <div class="header">
          <span class="title"></span>
          <span class="badge">Idle</span>
        </div>
        <div class="controls">
          <button class="listen-btn" type="button">Listen</button>
          <button class="talk-btn" type="button">Hold to Talk</button>
          <button class="unlock-btn" type="button">Unlock</button>
        </div>
        <div class="meta"></div>
        <div class="error"></div>
      </ha-card>
    `;

    const listenBtn = this.shadowRoot.querySelector(".listen-btn");
    const talkBtn = this.shadowRoot.querySelector(".talk-btn");
    const unlockBtn = this.shadowRoot.querySelector(".unlock-btn");

    listenBtn.addEventListener("click", () => {
      if (this._connected) {
        this._disconnect();
      } else {
        this._connect();
      }
    });

    talkBtn.addEventListener("pointerdown", (e) => {
      e.preventDefault();
      if (talkBtn.setPointerCapture) {
        try {
          talkBtn.setPointerCapture(e.pointerId);
        } catch (_) {}
      }
      this._startTalk();
    });
    const endTalk = (e) => {
      e.preventDefault();
      this._stopTalk();
    };
    talkBtn.addEventListener("pointerup", endTalk);
    talkBtn.addEventListener("pointercancel", endTalk);
    talkBtn.addEventListener("lostpointercapture", endTalk);

    unlockBtn.addEventListener("click", () => this._unlockDoor());
    this._updateStateUI();
  }

  _updateStateUI() {
    if (!this._rendered) return;
    const titleEl = this.shadowRoot.querySelector(".title");
    const badgeEl = this.shadowRoot.querySelector(".badge");
    const listenBtn = this.shadowRoot.querySelector(".listen-btn");
    const talkBtn = this.shadowRoot.querySelector(".talk-btn");
    const unlockBtn = this.shadowRoot.querySelector(".unlock-btn");
    const metaEl = this.shadowRoot.querySelector(".meta");
    const errEl = this.shadowRoot.querySelector(".error");

    titleEl.textContent = this._config.title || "Intercom Bridge";

    if (this._talking) {
      badgeEl.textContent = "Talking";
      badgeEl.className = "badge talking";
    } else if (this._connected) {
      badgeEl.textContent = "Listening";
      badgeEl.className = "badge live";
    } else {
      badgeEl.textContent = "Idle";
      badgeEl.className = "badge";
    }

    listenBtn.textContent = this._connected ? "Stop Listen" : "Listen";
    listenBtn.className = this._connected ? "listen-btn active" : "listen-btn";

    talkBtn.textContent = this._talking ? "Talking..." : "Hold to Talk";
    talkBtn.className = this._talking ? "talk-btn talking" : "talk-btn";

    if (this._config.lock_entity) {
      unlockBtn.style.display = "";
      const lockState = this._hass?.states?.[this._config.lock_entity]?.state;
      unlockBtn.disabled = this._unlocking;
      unlockBtn.textContent = this._unlocking
        ? "Unlocking..."
        : lockState === "unlocked"
          ? "Unlocked"
          : "Unlock";
    } else {
      unlockBtn.style.display = "none";
    }

    const metaParts = [];
    if (this._config.ring_entity && this._hass?.states?.[this._config.ring_entity]) {
      const ringState = this._hass.states[this._config.ring_entity];
      metaParts.push(`Ring: ${ringState.state}`);
    }
    metaEl.textContent = metaParts.join(" • ");
    metaEl.style.display = metaParts.length ? "" : "none";

    errEl.textContent = this._error || "";
    errEl.style.display = this._error ? "" : "none";
  }
}

customElements.define("intercom-bridge-card", IntercomBridgeCard);

window.customCards = window.customCards || [];
window.customCards.push({
  type: "intercom-bridge-card",
  name: "Intercom Bridge Card",
  description: "Two-way Listen, Hold-to-Talk, and Matter Door Unlock for Intercom Bridge.",
});
