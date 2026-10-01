import assert from "node:assert/strict";
import { describe, test } from "node:test";
import { boundedCanvasDimensions } from "../src/canvas-dimensions.ts";

describe("playground canvas dimensions", () => {
  test("preserve aspect ratio within the device limit", () => {
    assert.deepEqual(boundedCanvasDimensions(800, 600, 2, 8_192), [1_600, 1_200]);
    assert.deepEqual(boundedCanvasDimensions(10_000, 5_000, 2, 8_192), [8_192, 4_096]);
  });
});
