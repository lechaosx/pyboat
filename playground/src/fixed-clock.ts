export type FixedClock = {
  accumulatorSeconds: number;
};

export const fixedTickSeconds = 1 / 60;

export function advanceFixedClock(
  clock: FixedClock,
  elapsedSeconds: number,
  advanceTick: () => void,
): number {
  clock.accumulatorSeconds += Math.max(elapsedSeconds, 0);
  const ticks = Math.floor(clock.accumulatorSeconds / fixedTickSeconds);
  for (let tick = 0; tick < ticks; tick += 1) {
    advanceTick();
  }
  clock.accumulatorSeconds -= ticks * fixedTickSeconds;
  return ticks;
}

export function fixedClockFraction(clock: FixedClock): number {
  return clock.accumulatorSeconds / fixedTickSeconds;
}
