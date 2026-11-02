export function createPttHandlers(
  onStartTalk: () => Promise<void> | void,
  onStopTalk: () => Promise<void> | void,
) {
  let pressed = false;

  const handlePttDown = (event: Pick<PointerEvent, 'preventDefault'>) => {
    event.preventDefault();
    if (pressed) {
      return;
    }
    pressed = true;
    void onStartTalk();
  };

  const handlePttUp = (event: Pick<PointerEvent, 'preventDefault'>) => {
    event.preventDefault();
    if (!pressed) {
      return;
    }
    pressed = false;
    void onStopTalk();
  };

  return { handlePttDown, handlePttUp };
}
