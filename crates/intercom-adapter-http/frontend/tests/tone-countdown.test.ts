import { expect, test } from 'bun:test';
import { createToneCountdown } from '../src/components/toneCountdown';

test('tone countdown starts on first observation and counts down to zero', () => {
  // Arrange
  const update = createToneCountdown();

  // Act
  const remaining = [
    update(true, 1000),
    update(true, 2000),
    update(true, 60500),
    update(true, 61000),
    update(true, 62000),
  ];

  // Assert
  expect(remaining).toEqual([60, 59, 1, 0, 0]);
});

test('inactive tone hides and resets countdown for the next activation', () => {
  // Arrange
  const update = createToneCountdown();
  update(true, 1000);

  // Act
  const inactive = update(false, 10000);
  const restarted = update(true, 20000);

  // Assert
  expect(inactive).toBeUndefined();
  expect(restarted).toBe(60);
});
