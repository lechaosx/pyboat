// A camera `heightM` above mean sea level looking straight down through a
// rectilinear lens of this vertical field of view.
export type DroneCamera = { heightM: number; fieldOfViewDegrees: number };

export const droneFieldOfViewLimitsDegrees = { min: 0.01, max: 170 };

export function droneViewHeightM(camera: DroneCamera): number {
  return 2 * camera.heightM * Math.tan(camera.fieldOfViewDegrees * Math.PI / 360);
}

export function droneFieldOfViewDegrees(viewHeightM: number, heightM: number): number {
  return 2 * Math.atan(viewHeightM / (2 * heightM)) * 180 / Math.PI;
}

// A wheel step scales the camera's height at its field of view; with Ctrl it
// scales the field of view at its height, and with Shift it scales the field
// of view and moves the camera to keep the sea-level view height (a dolly
// zoom).
export function wheeledDroneCamera(
  camera: DroneCamera,
  scale: number,
  modifiers: { ctrlKey: boolean; shiftKey: boolean },
): DroneCamera {
  if (!modifiers.ctrlKey && !modifiers.shiftKey) {
    return { ...camera, heightM: camera.heightM * scale };
  }
  const fieldOfViewDegrees = Math.min(
    droneFieldOfViewLimitsDegrees.max,
    Math.max(droneFieldOfViewLimitsDegrees.min, camera.fieldOfViewDegrees * scale),
  );
  if (modifiers.ctrlKey) return { ...camera, fieldOfViewDegrees };
  return {
    heightM: droneViewHeightM(camera) / (2 * Math.tan(fieldOfViewDegrees * Math.PI / 360)),
    fieldOfViewDegrees,
  };
}

// The camera moved to `heightM`, with the field of view that keeps its
// sea-level view height.
export function droneCameraAtHeight(camera: DroneCamera, heightM: number): DroneCamera {
  return { heightM, fieldOfViewDegrees: droneFieldOfViewDegrees(droneViewHeightM(camera), heightM) };
}
