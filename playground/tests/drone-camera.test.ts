import assert from "node:assert/strict";
import { describe, test } from "node:test";
import { droneCameraAtHeight, droneViewHeightM, wheeledDroneCamera } from "../src/drone-camera.ts";

const camera = { heightM: 1000, fieldOfViewDegrees: 30 };

function assertClose(actual: number, expected: number): void {
  assert.ok(Math.abs(actual - expected) <= 1e-9 * Math.abs(expected), `${actual} != ${expected}`);
}

describe("playground drone camera", () => {
  test("a wheel step moves the camera at its field of view", () => {
    const wheeled = wheeledDroneCamera(camera, 2, { ctrlKey: false, shiftKey: false });

    assert.equal(wheeled.fieldOfViewDegrees, camera.fieldOfViewDegrees);
    assertClose(wheeled.heightM, 2000);
    assertClose(droneViewHeightM(wheeled), 2 * droneViewHeightM(camera));
  });

  test("a Ctrl+wheel step changes the field of view at the camera's height", () => {
    const wheeled = wheeledDroneCamera(camera, 2, { ctrlKey: true, shiftKey: false });

    assert.equal(wheeled.heightM, camera.heightM);
    assertClose(wheeled.fieldOfViewDegrees, 60);
  });

  test("a Shift+wheel step dolly zooms at the sea-level view height", () => {
    const wheeled = wheeledDroneCamera(camera, 0.5, { ctrlKey: false, shiftKey: true });

    assertClose(wheeled.fieldOfViewDegrees, 15);
    assertClose(droneViewHeightM(wheeled), droneViewHeightM(camera));
    assert.ok(wheeled.heightM > 2 * camera.heightM);
  });

  test("a wheel step keeps the field of view inside the lens's range", () => {
    for (const shiftKey of [false, true]) {
      const modifiers = { ctrlKey: !shiftKey, shiftKey };
      assert.equal(wheeledDroneCamera(camera, 100, modifiers).fieldOfViewDegrees, 170);
      assert.equal(wheeledDroneCamera(camera, 1e-9, modifiers).fieldOfViewDegrees, 0.01);
    }
  });

  test("moved to another height keeps the sea-level view height", () => {
    const moved = droneCameraAtHeight(camera, 250);

    assert.equal(moved.heightM, 250);
    assertClose(droneViewHeightM(moved), droneViewHeightM(camera));
  });
});
