import assert from "node:assert/strict";
import { test } from "node:test";
import { benchSettings, median, summarizeScenario } from "../src/bench.ts";

test("bench mode is opt-in and defaults to the native bench resolution", () => {
  assert.equal(benchSettings("?seed=1"), undefined);
  assert.deepEqual(benchSettings("?bench"), {
    width: 1920, height: 1080, warmupFrames: 5, timedFrames: 20,
  });
  assert.deepEqual(benchSettings("?bench&width=1280&height=720&warmup=0&frames=3"), {
    width: 1280, height: 720, warmupFrames: 0, timedFrames: 3,
  });
  assert.throws(() => benchSettings("?bench&frames=0"));
  assert.throws(() => benchSettings("?bench&width=1.5"));
});

test("median uses the middle sample or the mean of the middle pair", () => {
  assert.equal(median([3, 1, 2]), 2);
  assert.equal(median([4, 1, 3, 2]), 2.5);
  assert.throws(() => median([]));
});

test("per-pixel cost follows GPU timestamps only when every frame has them", () => {
  const passes = (totalMs: number) => ({
    fieldsMs: 0.5, occludersMs: 2, sceneMs: totalMs - 2.5, totalMs,
  });
  const timed = summarizeScenario("coast", [
    { submittedToDoneMs: 30, passes: passes(20) },
    { submittedToDoneMs: 50, passes: passes(40) },
    { submittedToDoneMs: 40, passes: passes(30) },
  ], 1_000_000);
  assert.deepEqual(timed, {
    id: "coast",
    submittedToDoneMs: 40,
    passes: { fieldsMs: 0.5, occludersMs: 2, sceneMs: 27.5, totalMs: 30 },
    nsPerPixel: 30,
  });

  const partial = summarizeScenario("coast", [
    { submittedToDoneMs: 30, passes: passes(20) },
    { submittedToDoneMs: 50 },
  ], 1_000_000);
  assert.deepEqual(partial, { id: "coast", submittedToDoneMs: 40, nsPerPixel: 40 });
});
