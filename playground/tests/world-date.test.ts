import { describe, expect, test } from "bun:test";
import {
  advanceProceduralTimeSeconds,
  maximumProceduralTimeSeconds,
  maximumWorldYear,
  worldDaySeconds,
  worldDateTimeSeconds,
  worldYearDays,
} from "../src/world-date";

describe("playground world date", () => {
  test("keeps fixed ticks representable at the maximum editable year", () => {
    const seconds = worldDateTimeSeconds(maximumWorldYear, 365, 23, 59, 59);
    const tickSeconds = 1 / 60;
    const representedTick = seconds + tickSeconds - seconds;

    expect(representedTick).toBeGreaterThan(0);
    expect(Math.abs(representedTick - tickSeconds)).toBeLessThanOrEqual(tickSeconds / 256);
    expect(seconds).toBeLessThanOrEqual(maximumProceduralTimeSeconds);
  });

  test("clamps unbounded years before converting them to time", () => {
    expect(worldDateTimeSeconds(Number.MAX_VALUE, 1, 0, 0, 0)).toBe(
      worldDateTimeSeconds(maximumWorldYear, 1, 0, 0, 0),
    );
  });

  test("clamps continued playback at the precise clock boundary", () => {
    expect(advanceProceduralTimeSeconds(maximumProceduralTimeSeconds - 1, 100_000)).toBe(
      maximumProceduralTimeSeconds,
    );
  });

  test("keeps dates reached throughout playback editable", () => {
    const terminalYear = Math.floor(
      maximumProceduralTimeSeconds / worldDaySeconds / worldYearDays,
    ) + 1;

    expect(maximumWorldYear).toBe(terminalYear);
    expect(worldDateTimeSeconds(terminalYear, 1, 0, 0, 0)).toBeGreaterThan(
      maximumProceduralTimeSeconds / 2,
    );
  });
});
