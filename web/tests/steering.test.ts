import assert from "node:assert/strict";
import { test } from "node:test";
import { pointerVector, worldSteering } from "../src/steering.ts";

test("pointer drag preserves fractional magnitude and maps screen up to world up", () => {
  assert.deepEqual(pointerVector(0, 0), [0, 0]);
  assert.deepEqual(pointerVector(0.25, -0.5), [0.25, 0.5]);
  assert.deepEqual(pointerVector(-0.0001, 0), [-0.0001, 0]);
});

test("pointer drag saturates at the unit disk while preserving direction", () => {
  assert.deepEqual(pointerVector(3, -4), [0.6, 0.8]);
  assert.deepEqual(pointerVector(-2, 0), [-1, 0]);
});

test("gamepad and pointer add in world coordinates even with a small stick input", () => {
  assert.deepEqual(worldSteering([0.0001, 0], [0.5, 0.5]), [0.5001, 0.5]);
  assert.deepEqual(worldSteering([0, -0.5], [0.5, 0.5]), [0.5, 1]);
  assert.deepEqual(worldSteering([0.5, 0.5], [-0.5, 0.5]), [0, 0]);
  assert.deepEqual(worldSteering([0.0001, 0], [0, 0]), [0.0001, 0]);
  assert.deepEqual(worldSteering([0, 0], [0.5, 0.5]), [0.5, 0.5]);
  assert.deepEqual(worldSteering(undefined, [0.5, 0.5]), [0.5, 0.5]);
});
