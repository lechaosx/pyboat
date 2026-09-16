import { describe, expect, test } from "bun:test";
import { CloudMotion, type CloudWorld } from "../src/cloud-motion";

const world: CloudWorld = { seed: "42", widthM: 64_000, heightM: 64_000 };

describe("playground cloud motion", () => {
  test("resets transport when seed or geometry changes", () => {
    const motion = new CloudMotion();
    motion.advance(world, 0, [2, 0], [0, 3]);
    expect(motion.advance(world, 10, [2, 0], [0, 3])).toEqual([20, 0, 0, 30]);

    expect(motion.advance({ ...world, seed: "43" }, 10, [2, 0], [0, 3]))
      .toEqual([0, 0, 0, 0]);
    expect(motion.advance({ ...world, widthM: 32_000 }, 10, [2, 0], [0, 3]))
      .toEqual([0, 0, 0, 0]);
  });
});
