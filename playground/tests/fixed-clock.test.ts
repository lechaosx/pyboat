import { describe, expect, test } from "bun:test";
import {
  advanceFixedClock,
  fixedClockFraction,
  type FixedClock,
} from "../src/fixed-clock";

function runFrames(frameCount: number, elapsedSeconds: number): number {
  const clock: FixedClock = { accumulatorSeconds: 0 };
  let ticks = 0;
  for (let frame = 0; frame < frameCount; frame += 1) {
    advanceFixedClock(clock, elapsedSeconds, () => {
      ticks += 1;
    });
  }
  return ticks;
}

describe("fixed playground clock", () => {
  test("advances 60 ticks per second independently of render rate", () => {
    expect(runFrames(120, 1 / 120)).toBe(60);
    expect(runFrames(60, 1 / 60)).toBe(60);
    expect(runFrames(30, 1 / 30)).toBe(60);
  });

  test("retains partial frame time", () => {
    const clock: FixedClock = { accumulatorSeconds: 0 };
    let ticks = 0;
    advanceFixedClock(clock, 1 / 120, () => { ticks += 1; });
    expect(ticks).toBe(0);
    advanceFixedClock(clock, 1 / 120, () => { ticks += 1; });
    expect(ticks).toBe(1);
  });

  test("reports progress toward the next tick for rendering", () => {
    const clock: FixedClock = { accumulatorSeconds: 0 };
    advanceFixedClock(clock, 1 / 240, () => {});
    expect(fixedClockFraction(clock)).toBeCloseTo(0.25);
  });

  test("catches up every tick after a long frame", () => {
    const clock: FixedClock = { accumulatorSeconds: 0 };
    let ticks = 0;
    advanceFixedClock(clock, 1, () => { ticks += 1; });
    expect(ticks).toBe(60);
  });
});
