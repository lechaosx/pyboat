import { describe, expect, test } from "bun:test";
import {
  maximumWorldYear,
  worldDateTimeSeconds,
} from "../src/world-date";

describe("playground world date", () => {
  test("keeps the maximum selectable year within exact tick conversion", () => {
    const seconds = worldDateTimeSeconds(maximumWorldYear, 365, 23, 59, 59);

    expect(Number.isSafeInteger(Math.floor(seconds * 60))).toBe(true);
  });

  test("clamps unbounded years before converting them to time", () => {
    expect(worldDateTimeSeconds(Number.MAX_VALUE, 1, 0, 0, 0)).toBe(
      worldDateTimeSeconds(maximumWorldYear, 1, 0, 0, 0),
    );
  });
});
