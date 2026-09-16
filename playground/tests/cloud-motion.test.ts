import { describe, expect, test } from "bun:test";
import { CloudMotion } from "../src/cloud-motion";

describe("playground cloud motion", () => {
  test("advances once by every frame interval, including the first", () => {
    const motion = new CloudMotion();

    expect(motion.advance(0.5, [2, -1], [-3, 4])).toEqual([1, -0.5, -1.5, 2]);
    expect(motion.advance(0.25, [2, -1], [-3, 4])).toEqual([1.5, -0.75, -2.25, 3]);
  });
});
