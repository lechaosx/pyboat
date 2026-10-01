import { fixedTicksPerSecond } from "./fixed-clock.ts";

export const worldDaySeconds = 86_400;
export const worldYearDays = 365;

const minimumClockStepsPerUlp = 256;
// Keep floating-point spacing at least 256 times finer than one fixed tick.
const maximumClockExponent = 52
  + Math.floor(Math.log2(1 / fixedTicksPerSecond / minimumClockStepsPerUlp))
  + 1;
export const maximumProceduralTimeSeconds = 2 ** maximumClockExponent - 1;
const maximumWholeDay = Math.floor(maximumProceduralTimeSeconds / worldDaySeconds);

export const maximumWorldYear = Math.floor(maximumWholeDay / worldYearDays) + 1;

export function advanceProceduralTimeSeconds(timeSeconds: number, elapsedSeconds: number): number {
  return Math.min(maximumProceduralTimeSeconds, timeSeconds + Math.max(0, elapsedSeconds));
}

export function worldDateTimeSeconds(
  year: number,
  day: number,
  hours: number,
  minutes: number,
  seconds: number,
): number {
  const finiteYear = Number.isFinite(year) ? year : 1;
  const boundedYear = Math.min(maximumWorldYear, Math.max(1, Math.floor(finiteYear)));
  const boundedDay = Math.min(worldYearDays, Math.max(1, Math.floor(day)));
  const dayIndex = (boundedYear - 1) * worldYearDays + boundedDay - 1;
  return Math.min(
    maximumProceduralTimeSeconds,
    dayIndex * worldDaySeconds
      + Math.min(23, Math.max(0, Math.floor(hours))) * 3600
      + Math.min(59, Math.max(0, Math.floor(minutes))) * 60
      + Math.min(59, Math.max(0, Math.floor(seconds))),
  );
}
