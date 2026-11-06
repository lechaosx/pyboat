import assert from "node:assert/strict";
import { describe, test } from "node:test";
import {
  droneAnchorShiftM,
  droneCameraAtHeight,
  droneViewHeightM,
  droneViewScaleM,
  wheeledDroneCamera,
  type DroneCamera,
} from "../src/drone-camera.ts";

const camera = { heightM: 1000, fieldOfViewDegrees: 30 };

function assertClose(actual: number, expected: number): void {
  assert.ok(Math.abs(actual - expected) <= 1e-9 * Math.abs(expected), `${actual} != ${expected}`);
}

// Where the eye ray through view point `u`, in image heights from the image
// centre, passes `elevationM`, as an offset from the point below the camera.
function rayOffsetM(camera: DroneCamera, u: number, elevationM: number): number {
  return u * 2 * Math.tan(camera.fieldOfViewDegrees * Math.PI / 360) * (camera.heightM - elevationM);
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

  test("a drag keeps the grabbed terrain point under the cursor", () => {
    for (const elevationM of [0, 300, 990]) {
      // The camera moves against the cursor by this much per image height.
      const shiftM = droneViewScaleM(camera, elevationM);

      assertClose(shiftM * 0.3, rayOffsetM(camera, 0.3, elevationM));
    }
  });

  test("every wheel step keeps the terrain point under the cursor there", () => {
    const u = 0.37;
    for (const elevationM of [0, 300]) {
      for (const scale of [0.5, 2]) {
        for (const [ctrlKey, shiftKey] of [[false, false], [true, false], [false, true]]) {
          const next = wheeledDroneCamera(camera, scale, { ctrlKey, shiftKey });
          const centerShiftM = u * droneAnchorShiftM(camera, next, elevationM);

          assertClose(
            rayOffsetM(camera, u, elevationM) - centerShiftM,
            rayOffsetM(next, u, elevationM),
          );
        }
      }
    }
  });

  test("a point not below the camera anchors input at sea level", () => {
    assertClose(droneViewScaleM(camera, camera.heightM), droneViewHeightM(camera));
    const below = droneCameraAtHeight(camera, 200);
    for (const [from, to] of [[camera, below], [below, camera]]) {
      assertClose(
        droneAnchorShiftM(from, to, 300),
        droneViewHeightM(from) - droneViewHeightM(to),
      );
    }
  });
});
