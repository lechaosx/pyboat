export const worldDaySeconds = 86_400;
export const worldYearDays = 365;

const maximumExactTick = Number.MAX_SAFE_INTEGER;
const ticksPerSecond = 60;
const maximumWholeDay = Math.floor(maximumExactTick / ticksPerSecond / worldDaySeconds);

export const maximumWorldYear = Math.floor((maximumWholeDay - worldYearDays + 1) / worldYearDays) + 1;

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
  return dayIndex * worldDaySeconds
    + Math.min(23, Math.max(0, Math.floor(hours))) * 3600
    + Math.min(59, Math.max(0, Math.floor(minutes))) * 60
    + Math.min(59, Math.max(0, Math.floor(seconds)));
}
