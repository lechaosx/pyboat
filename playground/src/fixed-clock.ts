export type FixedClock = {
  accumulatorSeconds: number;
};

export const fixedTickSeconds = 1 / 60;

export function advanceFixedClock(
  clock: FixedClock,
  elapsedSeconds: number,
  advanceTicks: (ticks: number) => void,
): number {
  clock.accumulatorSeconds += Math.max(elapsedSeconds, 0);
  const ticks = Math.floor(clock.accumulatorSeconds / fixedTickSeconds);
  if (ticks > 0) {
    advanceTicks(ticks);
  }
  clock.accumulatorSeconds -= ticks * fixedTickSeconds;
  return ticks;
}

export function fixedClockFraction(clock: FixedClock): number {
  return clock.accumulatorSeconds / fixedTickSeconds;
}
