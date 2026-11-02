import { expect, mock, test } from 'bun:test';
import * as handlers from '../src/components/pttHandlers';

type PointerInput = Pick<PointerEvent, 'preventDefault'>;
type PttHandlers = {
  handlePttDown: (event: PointerInput) => void;
  handlePttUp: (event: PointerInput) => void;
};

function setup() {
  const createPttHandlers = Reflect.get(handlers, 'createPttHandlers') as (
    start: () => Promise<void> | void,
    stop: () => Promise<void> | void,
  ) => PttHandlers;
  expect(createPttHandlers).toBeFunction();
  const start = mock(() => new Promise<void>(() => {}));
  const stop = mock(() => {});
  return { ...createPttHandlers(start, stop), start, stop };
}

test.each(['pointerup', 'pointercancel', 'pointerleave'])(
  '%s stops a local press even before start completes or status updates',
  (type) => {
    // Arrange
    const { handlePttDown, handlePttUp, start, stop } = setup();
    const event = { type, preventDefault: mock(() => {}) };

    // Act
    handlePttDown(event);
    handlePttUp(event);
    handlePttUp(event);

    // Assert
    expect(start).toHaveBeenCalledTimes(1);
    expect(stop).toHaveBeenCalledTimes(1);
    expect(event.preventDefault).toHaveBeenCalledTimes(3);
  },
);

test('release without a local press does not stop another session', () => {
  // Arrange
  const { handlePttUp, start, stop } = setup();

  // Act
  handlePttUp({ preventDefault() {} });

  // Assert
  expect(start).not.toHaveBeenCalled();
  expect(stop).not.toHaveBeenCalled();
});

test('repeated downs start once and a new press can start after release', () => {
  // Arrange
  const { handlePttDown, handlePttUp, start, stop } = setup();
  const event = { preventDefault() {} };

  // Act
  handlePttDown(event);
  handlePttDown(event);
  handlePttUp(event);
  handlePttDown(event);
  handlePttUp(event);

  // Assert
  expect(start).toHaveBeenCalledTimes(2);
  expect(stop).toHaveBeenCalledTimes(2);
});
