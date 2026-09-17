import { describe, expect, test } from "bun:test";
import { boundedCanvasDimensions } from "../src/canvas-dimensions";

describe("playground canvas dimensions", () => {
  test("preserve aspect ratio within the device limit", () => {
    expect(boundedCanvasDimensions(800, 600, 2, 8_192)).toEqual([1_600, 1_200]);
    expect(boundedCanvasDimensions(10_000, 5_000, 2, 8_192)).toEqual([8_192, 4_096]);
  });
});
