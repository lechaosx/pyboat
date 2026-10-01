export class CloudMotion {
  private readonly lowOffsetM: [number, number] = [0, 0];
  private readonly upperOffsetM: [number, number] = [0, 0];

  advance(
    elapsedSeconds: number,
    lowWindMS: [number, number],
    upperWindMS: [number, number],
  ): [number, number, number, number] {
    for (const axis of [0, 1] as const) {
      this.lowOffsetM[axis] += elapsedSeconds * lowWindMS[axis];
      this.upperOffsetM[axis] += elapsedSeconds * upperWindMS[axis];
    }
    return [...this.lowOffsetM, ...this.upperOffsetM];
  }
}
