import { describe, expect, test } from "bun:test";
import {
  advanceWorldTimeSeconds,
  maximumWorldTimeSeconds,
  maximumWorldYear,
  worldDateTimeSeconds,
} from "../src/world-date";

describe("playground world date", () => {
  test("keeps fixed ticks representable at the maximum selectable year", () => {
    const seconds = worldDateTimeSeconds(maximumWorldYear, 365, 23, 59, 59);
    const tickSeconds = 1 / 60;
    const representedTick = seconds + tickSeconds - seconds;

    expect(representedTick).toBeGreaterThan(0);
    expect(Math.abs(representedTick - tickSeconds)).toBeLessThanOrEqual(tickSeconds / 256);
    expect(seconds).toBeLessThan(maximumWorldTimeSeconds);
  });

  test("clamps unbounded years before converting them to time", () => {
    expect(worldDateTimeSeconds(Number.MAX_VALUE, 1, 0, 0, 0)).toBe(
      worldDateTimeSeconds(maximumWorldYear, 1, 0, 0, 0),
    );
  });

  test("clamps continued playback at the precise clock boundary", () => {
    expect(advanceWorldTimeSeconds(maximumWorldTimeSeconds - 1, 100_000)).toBe(
      maximumWorldTimeSeconds,
    );
  });
});
