const ULAW_BIAS = 0x84;
const ULAW_CLIP = 32635;

export function encodeMulawSample(pcm16: number): number {
  const sign = pcm16 < 0 ? 0x80 : 0x00;
  let mag = Math.abs(pcm16);
  if (mag > ULAW_CLIP) {
    mag = ULAW_CLIP;
  }
  mag += ULAW_BIAS;
  let exponent = 7;
  for (let expMask = 0x4000; (mag & expMask) === 0 && exponent > 0; exponent -= 1) {
    expMask >>= 1;
  }
  const mantissa = (mag >> (exponent + 3)) & 0x0f;
  return ~(sign | (exponent << 4) | mantissa) & 0xff;
}

export function decodeMulawSample(ulaw: number): number {
  const u = ~ulaw & 0xff;
  const sign = (u & 0x80) !== 0;
  const exponent = (u >> 4) & 0x07;
  const mantissa = u & 0x0f;
  const magnitude = (((mantissa << 3) + ULAW_BIAS) << exponent) - ULAW_BIAS;
  return sign ? -magnitude : magnitude;
}

export function decodeMulawToFloat32(bytes: Uint8Array): Float32Array<ArrayBuffer> {
  const out = new Float32Array(bytes.length);
  for (let i = 0; i < bytes.length; i += 1) {
    out[i] = decodeMulawSample(bytes[i]) / 32768;
  }
  return out;
}

export function encodeFloat32ToMulaw(samples: Float32Array): Uint8Array<ArrayBuffer> {
  const out = new Uint8Array(samples.length);
  for (let i = 0; i < samples.length; i += 1) {
    const clamped = Math.max(-1, Math.min(1, samples[i]));
    out[i] = encodeMulawSample(Math.round(clamped * 32767));
  }
  return out;
}
