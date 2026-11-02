export function createToneCountdown() {
  let deadline: number | undefined;

  // Status exposes tone_active, not its start time. This is an estimate from the
  // first active observation; the firmware alone enforces the 60-second cutoff.
  return (active: boolean, now: number): number | undefined => {
    if (!active) {
      deadline = undefined;
      return undefined;
    }
    deadline ??= now + 60000;
    return Math.max(0, Math.ceil((deadline - now) / 1000));
  };
}
