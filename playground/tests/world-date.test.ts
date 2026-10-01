import assert from "node:assert/strict";
import { describe, test } from "node:test";
import {
  advanceProceduralTimeSeconds,
  maximumProceduralTimeSeconds,
  maximumWorldYear,
  worldDaySeconds,
  worldDateTimeSeconds,
  worldYearDays,
} from "../src/world-date.ts";

describe("playground world date", () => {
  test("keeps fixed ticks representable at the maximum editable year", () => {
    const seconds = worldDateTimeSeconds(maximumWorldYear, 365, 23, 59, 59);
    const tickSeconds = 1 / 60;
    const representedTick = seconds + tickSeconds - seconds;

    assert.ok(representedTick > 0);
    assert.ok(Math.abs(representedTick - tickSeconds) <= tickSeconds / 256);
    assert.ok(seconds <= maximumProceduralTimeSeconds);
  });

  test("clamps unbounded years before converting them to time", () => {
    assert.equal(
      worldDateTimeSeconds(Number.MAX_VALUE, 1, 0, 0, 0),
      worldDateTimeSeconds(maximumWorldYear, 1, 0, 0, 0),
    );
  });

  test("clamps continued playback at the precise clock boundary", () => {
    assert.equal(
      advanceProceduralTimeSeconds(maximumProceduralTimeSeconds - 1, 100_000),
      maximumProceduralTimeSeconds,
    );
  });

  test("keeps dates reached throughout playback editable", () => {
    const terminalYear = Math.floor(
      maximumProceduralTimeSeconds / worldDaySeconds / worldYearDays,
    ) + 1;

    assert.equal(maximumWorldYear, terminalYear);
    assert.ok(
      worldDateTimeSeconds(terminalYear, 1, 0, 0, 0) > maximumProceduralTimeSeconds / 2,
    );
  });
});
