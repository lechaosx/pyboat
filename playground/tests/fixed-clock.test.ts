import assert from "node:assert/strict";
import { describe, test } from "node:test";
import {
  advanceFixedClock,
  fixedClockFraction,
  type FixedClock,
} from "../src/fixed-clock.ts";

function runFrames(frameCount: number, elapsedSeconds: number): number {
  const clock: FixedClock = { accumulatorSeconds: 0 };
  let ticks = 0;
  for (let frame = 0; frame < frameCount; frame += 1) {
    advanceFixedClock(clock, elapsedSeconds, (elapsedTicks) => {
      ticks += elapsedTicks;
    });
  }
  return ticks;
}

describe("fixed playground clock", () => {
  test("advances 60 ticks per second independently of render rate", () => {
    assert.equal(runFrames(120, 1 / 120), 60);
    assert.equal(runFrames(60, 1 / 60), 60);
    assert.equal(runFrames(30, 1 / 30), 60);
  });

  test("retains partial frame time", () => {
    const clock: FixedClock = { accumulatorSeconds: 0 };
    let ticks = 0;
    advanceFixedClock(clock, 1 / 120, (elapsedTicks) => { ticks += elapsedTicks; });
    assert.equal(ticks, 0);
    advanceFixedClock(clock, 1 / 120, (elapsedTicks) => { ticks += elapsedTicks; });
    assert.equal(ticks, 1);
  });

  test("reports progress toward the next tick for rendering", () => {
    const clock: FixedClock = { accumulatorSeconds: 0 };
    advanceFixedClock(clock, 1 / 240, () => {});
    assert.ok(Math.abs(fixedClockFraction(clock) - 0.25) < 0.005);
  });

  test("catches up every tick after a long frame", () => {
    const clock: FixedClock = { accumulatorSeconds: 0 };
    let ticks = 0;
    advanceFixedClock(clock, 1, (elapsedTicks) => { ticks += elapsedTicks; });
    assert.equal(ticks, 60);
  });

  test("batches a long suspension into one state update", () => {
    const clock: FixedClock = { accumulatorSeconds: 0 };
    let callbacks = 0;

    const ticks = advanceFixedClock(clock, 10, () => { callbacks += 1; });

    assert.equal(ticks, 600);
    assert.equal(callbacks, 1);
  });
});
