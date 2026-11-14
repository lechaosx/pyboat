export function pointerVector(dx: number, dy: number): [number, number] {
  const scale = Math.max(1, Math.hypot(dx, dy));
  return [dx / scale, dy === 0 ? 0 : -dy / scale];
}

export function worldSteering(
  gamepadAxes: readonly number[] | undefined,
  pointer: [number, number],
): [number, number] {
  const x = gamepadAxes?.[0] ?? 0;
  const y = -(gamepadAxes?.[1] ?? 0);
  return x === 0 && y === 0 ? pointer : [x, y === 0 ? 0 : y];
}
