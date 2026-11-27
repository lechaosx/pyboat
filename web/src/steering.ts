export function pointerVector(dx: number, dy: number): [number, number] {
  const scale = Math.max(1, Math.hypot(dx, dy));
  return [dx / scale, dy === 0 ? 0 : -dy / scale];
}

export function worldSteering(
  gamepadAxes: readonly number[] | undefined,
  pointer: [number, number],
): [number, number] {
  return [pointer[0] + (gamepadAxes?.[0] ?? 0), pointer[1] - (gamepadAxes?.[1] ?? 0)];
}
