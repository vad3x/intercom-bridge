import { expect, test } from 'bun:test';
import {
  decodeMulawSample,
  decodeMulawToFloat32,
  encodeFloat32ToMulaw,
  encodeMulawSample,
} from '../src/audio/mulaw';

test('G.711 reference values and all byte codes round trip', () => {
  // Arrange
  const vectors = [
    [0, 255],
    [1000, 206],
    [-1000, 78],
    [32767, 128],
    [-32768, 0],
  ];

  // Act
  const encoded = vectors.map(([pcm]) => encodeMulawSample(pcm));

  // Assert
  expect(encoded).toEqual(vectors.map(([, code]) => code));
  for (let code = 0; code < 256; code += 1) {
    expect(encodeMulawSample(decodeMulawSample(code))).toBe(code === 127 ? 255 : code);
  }
});

test('float conversion clamps and normalizes signed samples', () => {
  // Arrange
  const input = new Float32Array([-2, -1, 0, 1, 2]);

  // Act
  const bytes = encodeFloat32ToMulaw(input);
  const pcm = decodeMulawToFloat32(bytes);

  // Assert
  expect([...bytes]).toEqual([0, 0, 255, 128, 128]);
  expect([...pcm]).toEqual([-32124 / 32768, -32124 / 32768, 0, 32124 / 32768, 32124 / 32768]);
});
