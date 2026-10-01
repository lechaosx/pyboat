import assert from "node:assert/strict";
import { describe, test } from "node:test";
import { CloudMotion } from "../src/cloud-motion.ts";

describe("playground cloud motion", () => {
  test("advances once by every frame interval, including the first", () => {
    const motion = new CloudMotion();

    assert.deepEqual(motion.advance(0.5, [2, -1], [-3, 4]), [1, -0.5, -1.5, 2]);
    assert.deepEqual(motion.advance(0.25, [2, -1], [-3, 4]), [1.5, -0.75, -2.25, 3]);
  });
});
